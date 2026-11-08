#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Compare an Android firmware's memory map with the devicetree built into a kernel.

examples:
  # on rooted Android: adb shell "su -c 'cp /sys/firmware/fdt /sdcard/fdt.dtb'"; adb pull /sdcard/fdt.dtb
  tools/fw-memory-check.py fdt.dtb Image-tb323fu-tNN               # compare (Image gunzipped)
  tools/fw-memory-check.py fdt.dtb --extract memory-map.txt         # text map for an issue
  tools/fw-memory-check.py firmware/memory-map-zui-18.0.12.104.txt Image   # a kernel you built, against .104

The kernel takes /memory from the bootloader and /reserved-memory from its own devicetree. Two things go wrong when
a firmware update moves memory around: a firmware region inside /memory that the kernel does not reserve becomes
ordinary RAM for Linux, and a region the kernel reserves at an old address no longer matches the firmware.
docs/firmware-updates.md explains the output.

Inputs are a flattened devicetree (Android's /sys/firmware/fdt, a .dtb), a kernel Image with the devicetree built
in, or a text map written by --extract. Both sides must be for the same SoC (qcom,msm-id at the root): a devicetree
from another device is refused unless --force is given.
"""
from __future__ import annotations

import argparse
import collections
import enum
import struct
import sys
from dataclasses import dataclass, field
from pathlib import Path

FDT_MAGIC = 0xD00DFEED
FDT_MAGIC_BYTES = struct.pack('>I', FDT_MAGIC)
FDT_BEGIN_NODE = 1
FDT_END_NODE = 2
FDT_PROP = 3
FDT_NOP = 4
FDT_END = 9
FDT_HEADER = struct.Struct('>10I')
FdtHeader = collections.namedtuple('FdtHeader', 'magic total_size off_struct off_strings off_rsvmap version '
                                                'last_comp_version boot_cpuid size_strings size_struct')

MIB = 1 << 20


class InputError(Exception):
    """An input file that cannot be read as a devicetree, kernel image or text map."""


# Devicetree reading

Node = dict[str, bytes]


def parse_fdt(blob: bytes) -> dict[str, Node]:
    """Every node of a flattened devicetree, by path ('/', '/memory@80000000', ...), with its raw properties."""
    if len(blob) < FDT_HEADER.size:
        raise InputError('too short for a devicetree header')
    header = FdtHeader(*FDT_HEADER.unpack_from(blob))
    if header.magic != FDT_MAGIC:
        raise InputError('not a devicetree (wrong magic)')
    if (header.total_size > len(blob) or header.off_struct + header.size_struct > header.total_size
            or header.off_strings + header.size_strings > header.total_size):
        raise InputError('devicetree is truncated')
    strings = blob[header.off_strings:header.off_strings + header.size_strings]
    end_of_struct = header.off_struct + header.size_struct

    def take(fmt: str, pos: int) -> tuple:
        if pos + struct.calcsize(fmt) > end_of_struct:
            raise InputError('devicetree structure ends early')
        return struct.unpack_from(fmt, blob, pos)

    def c_string(data: bytes, pos: int) -> str:
        end = data.find(b'\0', pos)
        if end < 0:
            raise InputError('unterminated name in devicetree')
        return data[pos:end].decode('ascii', 'replace')

    nodes: dict[str, Node] = {}
    stack: list[str] = []
    pos = header.off_struct
    while True:
        (token,) = take('>I', pos)
        pos += 4
        if token == FDT_BEGIN_NODE:
            name = c_string(blob, pos)
            pos = align4(pos + len(name) + 1)
            stack.append(name)
            nodes[node_path(stack)] = {}
        elif token == FDT_END_NODE:
            if not stack:
                raise InputError('devicetree closes a node it never opened')
            stack.pop()
        elif token == FDT_PROP:
            length, name_offset = take('>II', pos)
            pos += 8
            if not stack or pos + length > end_of_struct:
                raise InputError('devicetree property outside a node or past the end')
            nodes[node_path(stack)][c_string(strings, name_offset)] = blob[pos:pos + length]
            pos = align4(pos + length)
        elif token == FDT_NOP:
            continue
        elif token == FDT_END:
            return nodes
        else:
            raise InputError(f'unknown devicetree token {token:#x} at offset {pos - 4:#x}')


def align4(pos: int) -> int:
    return (pos + 3) & ~3


def node_path(stack: list[str]) -> str:
    return '/' + '/'.join(stack[1:])


def devicetree_in(blob: bytes) -> bytes:
    """The devicetree itself, or the one with a /reserved-memory node built into a kernel Image."""
    if blob.startswith(FDT_MAGIC_BYTES):
        return blob
    pos = blob.find(FDT_MAGIC_BYTES)
    while 0 <= pos <= len(blob) - FDT_HEADER.size:
        (total_size,) = struct.unpack_from('>I', blob, pos + 4)
        candidate = blob[pos:pos + total_size]
        if len(candidate) == total_size:
            try:
                if '/reserved-memory' in parse_fdt(candidate):
                    return candidate
            except InputError:
                pass
        pos = blob.find(FDT_MAGIC_BYTES, pos + 4)
    raise InputError('no devicetree with /reserved-memory found in this file')


# The memory map

@dataclass(frozen=True, order=True)
class Span:
    """A physical address range [start, end)."""
    start: int
    end: int

    @property
    def size(self) -> int:
        return self.end - self.start

    def overlaps(self, other: Span) -> bool:
        return self.start < other.end and other.start < self.end

    def contains(self, other: Span) -> bool:
        return self.start <= other.start and other.end <= self.end

    def __str__(self) -> str:
        return f'0x{self.start:010x}-0x{self.end:010x}'


@dataclass(frozen=True)
class Region:
    """A /reserved-memory node: no spans means the system places it at boot (size and alloc-ranges only)."""
    name: str
    spans: tuple[Span, ...]
    flag: str  # 'no-map', 'reusable' or ''

    @property
    def first_address(self) -> int:
        return self.spans[0].start if self.spans else 1 << 64


@dataclass
class MemoryMap:
    ram: list[Span] = field(default_factory=list)
    regions: list[Region] = field(default_factory=list)
    model: str = ''
    compatible: tuple[str, ...] = ()
    msm_id: tuple[int, ...] = ()
    board_id: tuple[int, ...] = ()

    @property
    def soc_ids(self) -> set[int]:
        """The SoC ids of qcom,msm-id, a list of <soc-id version> pairs."""
        return set(self.msm_id[::2])

    def describe(self) -> str:
        ids = [f'{name} {format_cells(value)}' for name, value in (('msm-id', self.msm_id), ('board-id', self.board_id))
               if value]
        return f'{self.model or "(no model)"} ({", ".join(ids) or "no device ids"})'

    @property
    def fixed(self) -> list[Region]:
        return sorted((r for r in self.regions if r.spans), key=lambda r: r.first_address)

    @property
    def placed_at_boot(self) -> list[Region]:
        return [r for r in self.regions if not r.spans]


def read_cells(value: bytes | None, default: int) -> int:
    return struct.unpack('>I', value)[0] if value and len(value) == 4 else default


def read_words(value: bytes | None) -> tuple[int, ...]:
    return struct.unpack(f'>{len(value) // 4}I', value[:len(value) // 4 * 4]) if value else ()


def read_strings(value: bytes | None) -> tuple[str, ...]:
    return tuple(s.decode('utf-8', 'replace') for s in (value or b'').split(b'\0') if s)


def format_cells(cells: tuple[int, ...]) -> str:
    return '<' + ' '.join(f'0x{c:x}' for c in cells) + '>'


def read_reg(value: bytes | None, address_cells: int, size_cells: int) -> list[Span]:
    """The (address, size) pairs of a reg property; pairs of size 0 are left out."""
    if not value:
        return []
    words = read_words(value)
    step = address_cells + size_cells
    spans = []
    for i in range(0, len(words) - step + 1, step):
        address = join_cells(words[i:i + address_cells])
        size = join_cells(words[i + address_cells:i + step])
        if size:
            spans.append(Span(address, address + size))
    return spans


def join_cells(cells: tuple[int, ...]) -> int:
    value = 0
    for cell in cells:
        value = (value << 32) | cell
    return value


def memory_map_of(nodes: dict[str, Node]) -> MemoryMap:
    root = nodes.get('/', {})
    address_cells = read_cells(root.get('#address-cells'), 2)
    size_cells = read_cells(root.get('#size-cells'), 1)
    ram = []
    for path, props in nodes.items():
        if path == '/memory' or path.startswith('/memory@'):
            ram += read_reg(props.get('reg'), address_cells, size_cells)

    reserved = nodes.get('/reserved-memory', {})
    reserved_address_cells = read_cells(reserved.get('#address-cells'), address_cells)
    reserved_size_cells = read_cells(reserved.get('#size-cells'), size_cells)
    regions = []
    for path, props in nodes.items():
        parent, _, name = path.rpartition('/')
        if parent != '/reserved-memory' or not is_enabled(props):
            continue
        flag = 'no-map' if 'no-map' in props else 'reusable' if 'reusable' in props else ''
        spans = read_reg(props.get('reg'), reserved_address_cells, reserved_size_cells)
        regions.append(Region(name, tuple(spans), flag))
    model = read_strings(root.get('model'))
    return MemoryMap(sorted(ram), regions, model[0] if model else '', read_strings(root.get('compatible')),
                     read_words(root.get('qcom,msm-id')), read_words(root.get('qcom,board-id')))


def is_enabled(props: Node) -> bool:
    return props.get('status', b'okay\0').rstrip(b'\0') in (b'okay', b'ok')


# Text maps (--extract): the root's model and ids, /memory and /reserved-memory. The ids name the SoC and board
# type, not a single device.

ID_HEADERS = ('model', 'compatible', 'msm-id', 'board-id')
RAM_HEADER = '# /memory (from the bootloader)'
REGIONS_HEADER = '# /reserved-memory: name, ranges, flags (no ranges: placed at boot)'


def format_text_map(memory: MemoryMap) -> str:
    lines = []
    if memory.model:
        lines.append(f'# model: {memory.model}')
    if memory.compatible:
        lines.append(f'# compatible: {" ".join(memory.compatible)}')
    if memory.msm_id:
        lines.append(f'# msm-id: {format_cells(memory.msm_id)}')
    if memory.board_id:
        lines.append(f'# board-id: {format_cells(memory.board_id)}')
    lines.append(RAM_HEADER)
    lines += [f'{span} {span.size / MIB:10.2f} MiB' for span in memory.ram]
    lines.append(REGIONS_HEADER)
    for region in memory.fixed + memory.placed_at_boot:
        ranges = ' '.join(str(span) for span in region.spans) or 'dynamic'
        lines.append(f'{region.name:<44} {ranges} {region.flag}'.rstrip())
    return '\n'.join(lines) + '\n'


def parse_text_map(text: str) -> MemoryMap:
    memory = MemoryMap()
    section = None
    for number, line in enumerate(text.splitlines(), 1):
        key, colon, value = line.removeprefix('# ').partition(':')
        if line.startswith('# ') and colon and key in ID_HEADERS:
            value = value.strip()
            if key == 'model':
                memory.model = value
            elif key == 'compatible':
                memory.compatible = tuple(value.split())
            else:
                setattr(memory, key.replace('-', '_'), parse_cells(value, number))
        elif line.startswith('# /memory'):
            section = 'ram'
        elif line.startswith('# /reserved-memory'):
            section = 'regions'
        elif not line.strip() or line.startswith('#'):
            continue
        elif section == 'ram':
            memory.ram.append(parse_span(line.split()[0], number))
        elif section == 'regions':
            name, *rest = line.split()
            flag = rest.pop() if rest and rest[-1] in ('no-map', 'reusable') else ''
            spans = () if rest == ['dynamic'] else tuple(parse_span(word, number) for word in rest)
            memory.regions.append(Region(name, spans, flag))
        else:
            raise InputError(f'line {number}: text before the "{RAM_HEADER}" line')
    return memory


def parse_cells(text: str, line_number: int) -> tuple[int, ...]:
    try:
        return tuple(int(word, 16) for word in text.strip('<> ').split())
    except ValueError:
        raise InputError(f'line {line_number}: "{text}" is not a cell list like <0x295 0x20000>') from None


def parse_span(word: str, line_number: int) -> Span:
    try:
        start, end = (int(part, 16) for part in word.split('-'))
    except ValueError:
        raise InputError(f'line {line_number}: "{word}" is not a range like 0x80000000-0x80e00000') from None
    return Span(start, end)


def load_memory_map(path: Path, kernel: bool) -> MemoryMap:
    """A memory map from a devicetree, a kernel Image (kernel=True) or a text map."""
    blob = path.read_bytes()
    if not blob.startswith(FDT_MAGIC_BYTES) and blob.lstrip().startswith(b'#'):
        return parse_text_map(blob.decode('utf-8'))
    devicetree = devicetree_in(blob) if kernel else blob
    return memory_map_of(parse_fdt(devicetree))


# Comparison

class AndroidStatus(enum.Enum):
    OUTSIDE_RAM = 'outside /memory'
    RESERVED = 'reserved by the kernel too'
    EXPOSED = 'RAM: {mib:.2f} MiB not reserved'


@dataclass
class AndroidVerdict:
    region: Region
    status: AndroidStatus
    exposed: list[Span]

    @property
    def is_problem(self) -> bool:
        return self.status is AndroidStatus.EXPOSED and self.region.flag == 'no-map'

    def describe(self) -> str:
        return self.status.value.format(mib=sum(s.size for s in self.exposed) / MIB)


class KernelStatus(enum.Enum):
    SAME = 'same as {names}'
    WITHIN = 'within {names}'
    DIFFERS = 'DIFFERS: overlaps {names}'
    ABSENT_IN_RAM = 'not in the map, inside /memory'
    ABSENT = 'not in the map, outside /memory'


@dataclass
class KernelVerdict:
    region: Region
    span: Span
    status: KernelStatus
    android_names: list[str]

    @property
    def is_problem(self) -> bool:
        return self.status is KernelStatus.DIFFERS

    def describe(self) -> str:
        return self.status.value.format(names=', '.join(self.android_names))


def intersection(spans: list[Span], others: list[Span]) -> list[Span]:
    return [Span(max(a.start, b.start), min(a.end, b.end)) for a in spans for b in others if a.overlaps(b)]


def difference(spans: list[Span], cuts: list[Span]) -> list[Span]:
    for cut in cuts:
        remaining = []
        for span in spans:
            if not span.overlaps(cut):
                remaining.append(span)
                continue
            if span.start < cut.start:
                remaining.append(Span(span.start, cut.start))
            if cut.end < span.end:
                remaining.append(Span(cut.end, span.end))
        spans = remaining
    return spans


def judge_android_region(region: Region, ram: list[Span], kernel_spans: list[Span]) -> AndroidVerdict:
    in_ram = intersection(list(region.spans), ram)
    if not in_ram:
        return AndroidVerdict(region, AndroidStatus.OUTSIDE_RAM, [])
    exposed = difference(in_ram, kernel_spans)
    if not exposed:
        return AndroidVerdict(region, AndroidStatus.RESERVED, [])
    return AndroidVerdict(region, AndroidStatus.EXPOSED, exposed)


def judge_kernel_span(region: Region, span: Span, android: list[Region], ram: list[Span]) -> KernelVerdict:
    android_spans = [(r.name, s) for r in android for s in r.spans]
    for status, matches in (
        (KernelStatus.SAME, lambda s: s == span),
        (KernelStatus.WITHIN, lambda s: s.contains(span)),
        (KernelStatus.DIFFERS, lambda s: s.overlaps(span)),
    ):
        names = [name for name, s in android_spans if matches(s)]
        if names:
            return KernelVerdict(region, span, status, names)
    status = KernelStatus.ABSENT_IN_RAM if intersection([span], ram) else KernelStatus.ABSENT
    return KernelVerdict(region, span, status, [])


@dataclass
class Comparison:
    android: MemoryMap
    kernel: MemoryMap
    android_verdicts: list[AndroidVerdict]
    kernel_verdicts: list[KernelVerdict]

    @property
    def has_problems(self) -> bool:
        return any(v.is_problem for v in self.android_verdicts + self.kernel_verdicts)


def compare(android: MemoryMap, kernel: MemoryMap) -> Comparison:
    kernel_spans = [span for region in kernel.fixed for span in region.spans]
    android_verdicts = [judge_android_region(r, android.ram, kernel_spans) for r in android.fixed]
    kernel_verdicts = [judge_kernel_span(r, span, android.fixed, android.ram)
                       for r in kernel.fixed for span in r.spans]
    return Comparison(android, kernel, android_verdicts, kernel_verdicts)


# Report

def span_column(span: Span) -> str:
    return f'0x{span.start:09x}+{span.size:<9x}'


def report(result: Comparison) -> str:
    android, kernel = result.android, result.kernel
    lines = [
        f'Android: {android.describe()}',
        f'kernel:  {kernel.describe()}',
        f'/memory from the bootloader: {len(android.ram)} ranges, {sum(s.size for s in android.ram) / MIB:.0f} MiB',
        f'kernel /reserved-memory: {len(kernel.fixed)} regions with a fixed address',
        f'not compared (placed at boot, no fixed address): {len(android.placed_at_boot)} in Android, '
        f'{len(kernel.placed_at_boot)} in the kernel',
        '',
        f'{"Android region":<44} {"range":<25} {"flags":<9} in Linux',
    ]
    for verdict in result.android_verdicts:
        for i, span in enumerate(verdict.region.spans):
            name, flag, what = (verdict.region.name, verdict.region.flag, verdict.describe()) if i == 0 else ('', '', '')
            lines.append(f'{name:<44} {span_column(span)} {flag:<9} {what}'.rstrip())

    lines += ['', f'{"kernel region":<44} {"range":<25} in Android']
    for verdict in result.kernel_verdicts:
        lines.append(f'{verdict.region.name:<44} {span_column(verdict.span)} {verdict.describe()}')

    lines.append('')
    differing = [v.region.name for v in result.kernel_verdicts if v.is_problem]
    if differing:
        lines.append(f'Kernel regions that no longer match Android: {", ".join(differing)}')
    exposed = [v for v in result.android_verdicts if v.status is AndroidStatus.EXPOSED]
    if not exposed:
        lines.append('Every fixed Android region is outside /memory or reserved by the kernel.')
    else:
        lines.append('Inside /memory and not reserved by the kernel (Linux may use this memory):')
        for verdict in exposed:
            spans = ' '.join(str(s) for s in verdict.exposed)
            lines.append(f'  {verdict.region.name:<44} {verdict.region.flag:<9} {spans}')
        lines += ['',
                  'reusable regions are memory pools Android also uses as RAM until it lends them out;',
                  'a no-map region here is firmware memory Linux would overwrite.']
    return '\n'.join(lines) + '\n'


def device_mismatch(android: MemoryMap, kernel: MemoryMap) -> str:
    if android.soc_ids and kernel.soc_ids and not android.soc_ids & kernel.soc_ids:
        socs = ', '.join(f'0x{i:x}' for i in sorted(android.soc_ids))
        return f'this devicetree is not from a TB323FU (SoC id {socs}, the kernel has 0x{min(kernel.soc_ids):x})'
    return ''


def device_warnings(android: MemoryMap, kernel: MemoryMap) -> list[str]:
    warnings = []
    if not android.msm_id:
        warnings.append('the Android side has no qcom,msm-id, so it cannot be checked to be from a TB323FU')
    if not kernel.msm_id:
        warnings.append('the kernel side has no qcom,msm-id, so the SoC check is skipped')
    if android.compatible and android.compatible == kernel.compatible:
        warnings.append("the Android side has the kernel's own compatible: was it taken under Linux? "
                        "/sys/firmware/fdt has to come from Android")
    if android.board_id and kernel.board_id and android.board_id != kernel.board_id:
        warnings.append(f'board id differs (Android {format_cells(android.board_id)}, '
                        f'kernel {format_cells(kernel.board_id)})')
    return warnings


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog='exit status: 0 no problem found, 1 a no-map region exposed or a kernel region that differs, '
               '2 unreadable input or a devicetree from another SoC')
    parser.add_argument('android', type=Path, help="Android's devicetree (/sys/firmware/fdt) or a text map")
    parser.add_argument('kernel', type=Path, nargs='?',
                        help='kernel Image (gunzipped), its .dtb, or a text map; leave out with --extract')
    parser.add_argument('--extract', type=Path, metavar='OUT.txt',
                        help="write the Android side's ids, /memory and /reserved-memory as text (for an issue)")
    parser.add_argument('--force', action='store_true', help='compare even when the SoC ids do not match')
    args = parser.parse_args(argv)
    if args.kernel is None and args.extract is None:
        parser.error('give a kernel to compare with, or --extract')

    try:
        android = load_memory_map(args.android, kernel=False)
        if args.extract:
            args.extract.write_text(format_text_map(android), encoding='utf-8', newline='\n')
        if args.kernel is None:
            return 0
        kernel = load_memory_map(args.kernel, kernel=True)
    except (OSError, InputError) as error:
        print(f'{parser.prog}: {error}', file=sys.stderr)
        return 2

    problem = device_mismatch(android, kernel)
    if problem and not args.force:
        print(f'{parser.prog}: {problem}; --force compares anyway', file=sys.stderr)
        return 2
    for warning in [problem] + device_warnings(android, kernel):
        if warning:
            print(f'{parser.prog}: warning: {warning}', file=sys.stderr)

    result = compare(android, kernel)
    sys.stdout.write(report(result))
    return 1 if result.has_problems else 0


if __name__ == '__main__':
    sys.exit(main())
