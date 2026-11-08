# SPDX-License-Identifier: MIT
"""Tests for tools/fw-memory-check.py: python3 -m unittest discover -s tools/tests"""
from __future__ import annotations

import contextlib
import importlib.util
import io
import struct
import sys
import tempfile
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parent.parent
REPO = TOOLS.parent
spec = importlib.util.spec_from_file_location('fw_memory_check', TOOLS / 'fw-memory-check.py')
fmc = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = fmc
spec.loader.exec_module(fmc)

MIB = 1 << 20


def build_fdt(tree: dict) -> bytes:
    """A flattened devicetree from {'props': {...}, 'children': {name: tree}}; values are bytes or None (empty)."""
    strings = bytearray()
    string_offsets: dict[str, int] = {}
    body = bytearray()

    def string_offset(name: str) -> int:
        if name not in string_offsets:
            string_offsets[name] = len(strings)
            strings.extend(name.encode() + b'\0')
        return string_offsets[name]

    def pad(data: bytearray) -> None:
        data.extend(b'\0' * (-len(data) % 4))

    def emit(name: str, node: dict) -> None:
        body.extend(struct.pack('>I', fmc.FDT_BEGIN_NODE))
        body.extend(name.encode() + b'\0')
        pad(body)
        for prop, value in node.get('props', {}).items():
            value = value or b''
            body.extend(struct.pack('>III', fmc.FDT_PROP, len(value), string_offset(prop)))
            body.extend(value)
            pad(body)
        for child, sub in node.get('children', {}).items():
            emit(child, sub)
        body.extend(struct.pack('>I', fmc.FDT_END_NODE))

    emit('', tree)
    body.extend(struct.pack('>I', fmc.FDT_END))
    rsvmap = b'\0' * 16
    off_rsvmap = 40
    off_struct = off_rsvmap + len(rsvmap)
    off_strings = off_struct + len(body)
    total = off_strings + len(strings)
    header = struct.pack('>10I', fmc.FDT_MAGIC, total, off_struct, off_strings, off_rsvmap, 17, 16, 0,
                         len(strings), len(body))
    return header + rsvmap + bytes(body) + bytes(strings)


def cells(*values: int) -> bytes:
    return struct.pack(f'>{len(values)}I', *values)


def reg(*pairs: tuple[int, int]) -> bytes:
    """reg with two address and two size cells."""
    return b''.join(cells(a >> 32, a & 0xFFFFFFFF, s >> 32, s & 0xFFFFFFFF) for a, s in pairs)


def devicetree(ram: list[tuple[int, int]], regions: dict[str, dict], root: dict | None = None) -> bytes:
    return build_fdt({
        'props': {'#address-cells': cells(2), '#size-cells': cells(2), **(root or {})},
        'children': {
            'memory': {'props': {'device_type': b'memory\0', 'reg': reg(*ram)}},
            'reserved-memory': {
                'props': {'#address-cells': cells(2), '#size-cells': cells(2), 'ranges': None},
                'children': {name: {'props': props} for name, props in regions.items()},
            },
        },
    })


def no_map(address: int, size: int) -> dict:
    return {'reg': reg((address, size)), 'no-map': None}


def reusable(address: int, size: int) -> dict:
    return {'reg': reg((address, size)), 'reusable': None}


def ids(model: str, compatible: list[str], msm_id: tuple[int, ...], board_id: tuple[int, ...] = (0x2000B, 0)) -> dict:
    return {'model': model.encode() + b'\0', 'compatible': b''.join(c.encode() + b'\0' for c in compatible),
            'qcom,msm-id': cells(*msm_id), 'qcom,board-id': cells(*board_id)}


# what Android 18.0.12.104 and the project's kernel have at the root
ANDROID_IDS = ids('Qualcomm Technologies, Inc. Canoe QRD + ST54L + Pandeiro', ['qcom,canoe-qrd', 'qcom,canoe'],
                  (0x295, 0x20000, 0x294, 0x10000, 0x1000295, 0x10000))
KERNEL_IDS = ids('Lenovo Legion Tab Y700 5th Gen', ['lenovo,baldur', 'qcom,kaanapali'], (0x295, 0x20000))

# RAM 0x80000000-0x90000000 with a hole for firmware at 0x81000000-0x82000000, as a bootloader hands it over
RAM = [(0x80000000, 0x1000000), (0x82000000, 0xE000000)]


def compare(android_regions: dict, kernel_regions: dict):
    android = fmc.memory_map_of(fmc.parse_fdt(devicetree(RAM, android_regions)))
    kernel = fmc.memory_map_of(fmc.parse_fdt(devicetree([], kernel_regions)))
    return fmc.compare(android, kernel)


class ParseTest(unittest.TestCase):
    def test_reads_memory_and_regions(self):
        memory = fmc.memory_map_of(fmc.parse_fdt(devicetree(RAM, {
            'fw@81000000': no_map(0x81000000, 0x1000000),
            'pool@84000000': reusable(0x84000000, 0x400000),
            'cma': {'size': cells(0, 0x1000000), 'reusable': None},
            'off@85000000': {**no_map(0x85000000, 0x1000), 'status': b'disabled\0'},
        })))
        self.assertEqual(memory.ram, [fmc.Span(0x80000000, 0x81000000), fmc.Span(0x82000000, 0x90000000)])
        self.assertEqual([(r.name, r.flag) for r in memory.fixed],
                         [('fw@81000000', 'no-map'), ('pool@84000000', 'reusable')])
        self.assertEqual([r.name for r in memory.placed_at_boot], ['cma'])

    def test_reads_device_ids(self):
        memory = fmc.memory_map_of(fmc.parse_fdt(devicetree(RAM, {}, ANDROID_IDS)))
        self.assertEqual(memory.model, 'Qualcomm Technologies, Inc. Canoe QRD + ST54L + Pandeiro')
        self.assertEqual(memory.compatible, ('qcom,canoe-qrd', 'qcom,canoe'))
        self.assertEqual(memory.soc_ids, {0x295, 0x294, 0x1000295})
        self.assertEqual(memory.board_id, (0x2000B, 0))

    def test_truncated_devicetree(self):
        blob = devicetree(RAM, {'fw@81000000': no_map(0x81000000, 0x1000000)})
        with self.assertRaises(fmc.InputError):
            fmc.parse_fdt(blob[:len(blob) // 2])

    def test_not_a_devicetree(self):
        with self.assertRaises(fmc.InputError):
            fmc.parse_fdt(b'\0' * 64)

    def test_devicetree_inside_a_kernel_image(self):
        dtb = devicetree([], {'fw@81000000': no_map(0x81000000, 0x1000000)})
        image = b'ARM\x64' + b'\x11' * 1000 + dtb + b'\x22' * 500
        self.assertEqual(fmc.devicetree_in(image), dtb)

    def test_kernel_image_without_a_devicetree(self):
        with self.assertRaises(fmc.InputError):
            fmc.devicetree_in(b'\x11' * 4096)


class VerdictTest(unittest.TestCase):
    def android_status(self, result, name):
        return next(v for v in result.android_verdicts if v.region.name == name).status

    def kernel_status(self, result, name):
        return next(v for v in result.kernel_verdicts if v.region.name == name).status

    def test_firmware_cut_out_of_ram_and_matching_kernel(self):
        result = compare({'fw@81000000': no_map(0x81000000, 0x1000000)},
                         {'fw@81000000': no_map(0x81000000, 0x1000000)})
        self.assertIs(self.android_status(result, 'fw@81000000'), fmc.AndroidStatus.OUTSIDE_RAM)
        self.assertIs(self.kernel_status(result, 'fw@81000000'), fmc.KernelStatus.SAME)
        self.assertFalse(result.has_problems)

    def test_no_map_region_in_ram_not_reserved_is_a_problem(self):
        result = compare({'new-fw@88000000': no_map(0x88000000, 0x100000)}, {})
        verdict = result.android_verdicts[0]
        self.assertIs(verdict.status, fmc.AndroidStatus.EXPOSED)
        self.assertEqual(verdict.exposed, [fmc.Span(0x88000000, 0x88100000)])
        self.assertTrue(result.has_problems)

    def test_reusable_region_in_ram_is_reported_but_no_problem(self):
        result = compare({'pool@84000000': reusable(0x84000000, 0x400000)}, {})
        self.assertIs(result.android_verdicts[0].status, fmc.AndroidStatus.EXPOSED)
        self.assertFalse(result.has_problems)

    def test_region_reserved_by_the_kernel(self):
        result = compare({'fw@88000000': no_map(0x88000000, 0x100000)},
                         {'fw@88000000': no_map(0x88000000, 0x100000)})
        self.assertIs(self.android_status(result, 'fw@88000000'), fmc.AndroidStatus.RESERVED)
        self.assertFalse(result.has_problems)

    def test_partly_reserved_region_exposes_the_rest(self):
        result = compare({'fw@88000000': no_map(0x88000000, 0x200000)},
                         {'fw@88000000': no_map(0x88000000, 0x100000)})
        self.assertEqual(result.android_verdicts[0].exposed, [fmc.Span(0x88100000, 0x88200000)])
        self.assertTrue(result.has_problems)

    def test_kernel_region_within_a_larger_android_region(self):
        result = compare({'pdp@81000000': no_map(0x81000000, 0x200000)},
                         {'pdp@81100000': no_map(0x81100000, 0x100000)})
        self.assertIs(self.kernel_status(result, 'pdp@81100000'), fmc.KernelStatus.WITHIN)
        self.assertFalse(result.has_problems)

    def test_moved_region_differs(self):
        result = compare({'mpss@81100000': no_map(0x81100000, 0x800000)},
                         {'mpss@81000000': no_map(0x81000000, 0x800000)})
        self.assertIs(self.kernel_status(result, 'mpss@81000000'), fmc.KernelStatus.DIFFERS)
        self.assertTrue(result.has_problems)

    def test_kernel_only_regions(self):
        result = compare({}, {'rmtfs@88000000': no_map(0x88000000, 0x400000),
                              'splash@81800000': no_map(0x81800000, 0x100000)})
        self.assertIs(self.kernel_status(result, 'rmtfs@88000000'), fmc.KernelStatus.ABSENT_IN_RAM)
        self.assertIs(self.kernel_status(result, 'splash@81800000'), fmc.KernelStatus.ABSENT)
        self.assertFalse(result.has_problems)

    def test_report_lists_every_span(self):
        result = compare({'two@88000000': {'reg': reg((0x88000000, 0x1000), (0x89000000, 0x1000)), 'no-map': None}},
                         {})
        text = fmc.report(result)
        self.assertIn('0x088000000', text)
        self.assertIn('0x089000000', text)


class TextMapTest(unittest.TestCase):
    def test_round_trip(self):
        memory = fmc.memory_map_of(fmc.parse_fdt(devicetree(RAM, {
            'fw@81000000': no_map(0x81000000, 0x1000000),
            'pool@84000000': reusable(0x84000000, 0x400000),
            'plain@86000000': {'reg': reg((0x86000000, 0x1000))},
            'cma': {'size': cells(0, 0x1000000), 'reusable': None},
        })))
        again = fmc.parse_text_map(fmc.format_text_map(memory))
        self.assertEqual(again.ram, memory.ram)
        self.assertEqual(again.model, '')
        self.assertEqual(sorted(again.regions, key=lambda r: r.name), sorted(memory.regions, key=lambda r: r.name))

    def test_round_trip_of_ids(self):
        memory = fmc.memory_map_of(fmc.parse_fdt(devicetree(RAM, {}, ANDROID_IDS)))
        again = fmc.parse_text_map(fmc.format_text_map(memory))
        self.assertEqual((again.model, again.compatible, again.msm_id, again.board_id),
                         (memory.model, memory.compatible, memory.msm_id, memory.board_id))

    def test_bad_cells(self):
        with self.assertRaises(fmc.InputError):
            fmc.parse_text_map('# msm-id: <0x295 zz>\n# /memory (from the bootloader)\n')

    def test_bad_range(self):
        with self.assertRaises(fmc.InputError):
            fmc.parse_text_map('# /memory (from the bootloader)\n0x80000000-zz 1 MiB\n')

    def test_reference_map_of_zui_104(self):
        memory = fmc.parse_text_map((REPO / 'firmware/memory-map-zui-18.0.12.104.txt').read_text(encoding='utf-8'))
        self.assertEqual(len(memory.ram), 20)
        self.assertEqual(memory.board_id, (0x2000B, 0))
        self.assertIn(0x295, memory.soc_ids)
        names = {r.name: r for r in memory.fixed}
        self.assertEqual(names['adspslpi_region@a1400000'].spans, (fmc.Span(0xA1400000, 0xA5A00000),))
        # the bootloader leaves every fixed no-map region out of /memory on this firmware
        exposed = [r.name for r in memory.fixed if r.flag == 'no-map' and fmc.intersection(list(r.spans), memory.ram)]
        self.assertEqual(exposed, [])


class CommandLineTest(unittest.TestCase):
    def run_main(self, *args: str) -> tuple[int, str, str]:
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            try:
                status = fmc.main(list(args))
            except SystemExit as exit_:
                status = exit_.code
        return status, out.getvalue(), err.getvalue()

    def test_exit_status_and_extract(self):
        with tempfile.TemporaryDirectory() as tmp:
            android = Path(tmp, 'fdt.dtb')
            kernel = Path(tmp, 'kernel.dtb')
            extract = Path(tmp, 'map.txt')
            android.write_bytes(devicetree(RAM, {'new@88000000': no_map(0x88000000, 0x1000)}))
            kernel.write_bytes(devicetree([], {}))
            status, out, _ = self.run_main('--extract', str(extract), str(android), str(kernel))
            self.assertEqual(status, 1)
            self.assertIn('new@88000000', out)
            self.assertEqual(extract.read_bytes().count(b'\r'), 0)
            status, _, _ = self.run_main(str(extract), str(kernel))
            self.assertEqual(status, 1)

    def run_ids(self, android_ids: dict, kernel_ids: dict, *flags: str) -> tuple[int, str, str]:
        with tempfile.TemporaryDirectory() as tmp:
            android = Path(tmp, 'fdt.dtb')
            kernel = Path(tmp, 'kernel.dtb')
            android.write_bytes(devicetree(RAM, {}, android_ids))
            kernel.write_bytes(devicetree([], {}, kernel_ids))
            return self.run_main(*flags, str(android), str(kernel))

    def test_same_soc_in_any_msm_id_pair(self):
        status, out, err = self.run_ids(ANDROID_IDS, KERNEL_IDS)
        self.assertEqual(status, 0)
        self.assertEqual(err, '')
        self.assertTrue(out.startswith('Android: Qualcomm Technologies'))
        self.assertIn('kernel:  Lenovo Legion Tab Y700 5th Gen (msm-id <0x295 0x20000>', out)
        status, _, _ = self.run_ids(ids('other', ['x'], (0x1, 0x10000, 0x295, 0x20000)), KERNEL_IDS)
        self.assertEqual(status, 0)

    def test_other_soc_is_refused_unless_forced(self):
        other = ids('Some phone', ['vendor,phone'], (0x1F4, 0x10000))
        status, out, err = self.run_ids(other, KERNEL_IDS)
        self.assertEqual((status, out), (2, ''))
        self.assertIn('not from a TB323FU (SoC id 0x1f4', err)
        status, out, err = self.run_ids(other, KERNEL_IDS, '--force')
        self.assertEqual(status, 0)
        self.assertIn('warning: this devicetree is not from a TB323FU', err)
        self.assertIn('Android: Some phone', out)

    def test_other_board_warns(self):
        status, _, err = self.run_ids(ids('x', ['y'], (0x295, 0x20000), (0x10008, 0)), KERNEL_IDS)
        self.assertEqual(status, 0)
        self.assertIn('board id differs (Android <0x10008 0x0>, kernel <0x2000b 0x0>)', err)

    def test_devicetree_taken_under_linux_warns(self):
        status, _, err = self.run_ids(KERNEL_IDS, KERNEL_IDS)
        self.assertEqual(status, 0)
        self.assertIn('taken under Linux', err)

    def test_no_ids_warns(self):
        status, out, err = self.run_ids({}, KERNEL_IDS)
        self.assertEqual(status, 0)
        self.assertIn('Android: (no model) (no device ids)', out)
        self.assertIn('Android side has no qcom,msm-id', err)

    def test_unreadable_input(self):
        with tempfile.TemporaryDirectory() as tmp:
            bad = Path(tmp, 'bad.dtb')
            bad.write_bytes(b'\0' * 100)
            status, _, err = self.run_main(str(bad), str(bad))
            self.assertEqual(status, 2)
            self.assertIn('not a devicetree', err)

    def test_kernel_or_extract_required(self):
        status, _, _ = self.run_main('fdt.dtb')
        self.assertEqual(status, 2)


if __name__ == '__main__':
    unittest.main()
