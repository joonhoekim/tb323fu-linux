// SPDX-License-Identifier: GPL-2.0
/*
 * gpu-probe: is the Adreno alive? (initramfs boot summary)
 *
 * No Mesa in the initramfs, so this talks to drm/msm directly:
 * reads the chip id and the GPU's always-on counter (twice -- it has to
 * move), then submits one CP_MEM_WRITE packet that stores a magic value into
 * a buffer and waits for the fence. The value turning up in the buffer means
 * the command processor (SQE) fetched and executed our stream.
 *
 *   gpu-probe [/dev/dri/renderD128]
 *
 * Build: aarch64-linux-gnu-gcc -static -O2 -I<out>/usr/include   (make O=<out> headers_install) gpu-probe.c
 */
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>
#include <drm/msm_drm.h>

#define CP_TYPE7_PKT	0x70000000
#define CP_NOP		0x10
#define CP_MEM_WRITE	0x3d
#define MAGIC		0xba1d0840

static uint32_t odd_parity(uint32_t v)
{
	v ^= v >> 16;
	v ^= v >> 8;
	v ^= v >> 4;
	v &= 0xf;
	return (~0x6996 >> v) & 1;
}

static uint32_t pkt7(uint32_t op, uint32_t cnt)
{
	return CP_TYPE7_PKT | cnt | (odd_parity(cnt) << 15) |
	       ((op & 0x7f) << 16) | (odd_parity(op) << 23);
}

static int param(int fd, uint32_t p, uint64_t *v)
{
	struct drm_msm_param req = { .pipe = MSM_PIPE_3D0, .param = p };
	int ret = ioctl(fd, DRM_IOCTL_MSM_GET_PARAM, &req);

	*v = req.value;
	return ret;
}

static int bo_new(int fd, uint32_t size, uint32_t *handle, uint64_t *iova, void **map)
{
	struct drm_msm_gem_new n = { .size = size, .flags = MSM_BO_WC };
	struct drm_msm_gem_info i = { 0 };

	if (ioctl(fd, DRM_IOCTL_MSM_GEM_NEW, &n))
		return -1;
	*handle = n.handle;
	i.handle = n.handle;
	i.info = MSM_INFO_GET_IOVA;
	if (ioctl(fd, DRM_IOCTL_MSM_GEM_INFO, &i))
		return -1;
	*iova = i.value;
	i.info = MSM_INFO_GET_OFFSET;
	i.value = 0;
	if (ioctl(fd, DRM_IOCTL_MSM_GEM_INFO, &i))
		return -1;
	*map = mmap(NULL, size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, i.value);
	return *map == MAP_FAILED ? -1 : 0;
}

int main(int argc, char **argv)
{
	const char *path = argc > 1 ? argv[1] : "/dev/dri/renderD128";
	uint64_t chip, gmem, t0, t1, cmd_iova, dst_iova;
	uint32_t cmd_h, dst_h, *cmd, *dst, n = 0;
	struct drm_msm_gem_submit_bo bos[2];
	struct drm_msm_gem_submit_cmd c;
	struct drm_msm_gem_submit s;
	struct drm_msm_wait_fence w;
	struct timespec now;
	void *m;
	int fd;

	fd = open(path, O_RDWR);
	if (fd < 0) {
		perror(path);
		return 1;
	}
	if (param(fd, MSM_PARAM_CHIP_ID, &chip) || param(fd, MSM_PARAM_GMEM_SIZE, &gmem)) {
		perror("GET_PARAM (is the GPU loaded?)");
		return 1;
	}
	printf("chip id %016llx, gmem %llu KiB\n", (unsigned long long)chip,
	       (unsigned long long)gmem >> 10);

	if (param(fd, MSM_PARAM_TIMESTAMP, &t0) == 0) {
		usleep(100000);
		param(fd, MSM_PARAM_TIMESTAMP, &t1);
		printf("always-on counter %llu -> %llu (+%llu in 100 ms)\n",
		       (unsigned long long)t0, (unsigned long long)t1,
		       (unsigned long long)(t1 - t0));
	} else {
		printf("TIMESTAMP: %s\n", strerror(errno));
	}

	if (bo_new(fd, 4096, &cmd_h, &cmd_iova, &m))
		return perror("cmd bo"), 1;
	cmd = m;
	if (bo_new(fd, 4096, &dst_h, &dst_iova, &m))
		return perror("dst bo"), 1;
	dst = m;
	dst[0] = 0;

	cmd[n++] = pkt7(CP_MEM_WRITE, 3);
	cmd[n++] = (uint32_t)dst_iova;
	cmd[n++] = (uint32_t)(dst_iova >> 32);
	cmd[n++] = MAGIC;
	cmd[n++] = pkt7(CP_NOP, 0);

	memset(bos, 0, sizeof(bos));
	bos[0].handle = cmd_h;
	bos[0].flags = MSM_SUBMIT_BO_READ;
	bos[0].presumed = cmd_iova;
	bos[1].handle = dst_h;
	bos[1].flags = MSM_SUBMIT_BO_WRITE;
	bos[1].presumed = dst_iova;

	memset(&c, 0, sizeof(c));
	c.type = MSM_SUBMIT_CMD_BUF;
	c.submit_idx = 0;
	c.submit_offset = 0;
	c.size = n * 4;

	memset(&s, 0, sizeof(s));
	s.flags = MSM_PIPE_3D0;
	s.nr_bos = 2;
	s.bos = (uintptr_t)bos;
	s.nr_cmds = 1;
	s.cmds = (uintptr_t)&c;
	if (ioctl(fd, DRM_IOCTL_MSM_GEM_SUBMIT, &s))
		return perror("SUBMIT"), 1;
	printf("submitted fence %u (cmd iova %llx, dst iova %llx)\n", s.fence,
	       (unsigned long long)cmd_iova, (unsigned long long)dst_iova);

	clock_gettime(CLOCK_MONOTONIC, &now);
	memset(&w, 0, sizeof(w));
	w.fence = s.fence;
	w.timeout.tv_sec = now.tv_sec + 3;
	w.timeout.tv_nsec = now.tv_nsec;
	if (ioctl(fd, DRM_IOCTL_MSM_WAIT_FENCE, &w)) {
		printf("WAIT_FENCE: %s -- dst[0] = %08x\n", strerror(errno), dst[0]);
		return 1;
	}
	printf("fence signalled, dst[0] = %08x -> %s\n", dst[0],
	       dst[0] == MAGIC ? "GPU EXECUTED OUR COMMANDS" : "value did not arrive");
	return dst[0] == MAGIC ? 0 : 1;
}
