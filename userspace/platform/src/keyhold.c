// SPDX-License-Identifier: MIT
/*
 * keyhold: wait until a set of keys has been held together for N seconds.
 * Used for the emergency "back to Android" chord -- volume up + volume down
 * held 10 s -- in the initramfs and on the Linux root filesystem.
 *
 *   keyhold SECONDS NAME:CODE [NAME:CODE ...]
 *   keyhold 10 pmic_resin:114 gpio-keys:115
 *
 * NAME is the input device name (EVIOCGNAME), CODE the key code. Polls the
 * key state (EVIOCGKEY) five times a second and prints one line per change:
 * "hold" when every key is down, "release" when one comes up before the time,
 * "done" after SECONDS held (then exits 0). Exits 1 if a device is missing.
 *
 * Build: aarch64-linux-gnu-gcc -static -O2 -o keyhold keyhold.c
 */
#include <fcntl.h>
#include <linux/input.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <time.h>
#include <unistd.h>

#define MAXKEYS 8

static int open_by_name(const char *name)
{
	char path[32], n[128];
	int i, fd;

	for (i = 0; i < 64; i++) {
		snprintf(path, sizeof(path), "/dev/input/event%d", i);
		fd = open(path, O_RDONLY | O_CLOEXEC);
		if (fd < 0)
			continue;
		if (ioctl(fd, EVIOCGNAME(sizeof(n)), n) > 0 && !strcmp(n, name))
			return fd;
		close(fd);
	}
	return -1;
}

static long now_ms(void)
{
	struct timespec t;

	clock_gettime(CLOCK_MONOTONIC, &t);
	return t.tv_sec * 1000 + t.tv_nsec / 1000000;
}

int main(int argc, char **argv)
{
	int fd[MAXKEYS], code[MAXKEYS], n = 0, i, all;
	long need, since = -1;
	unsigned char k[KEY_MAX / 8 + 1];

	if (argc < 3) {
		fprintf(stderr, "usage: %s SECONDS NAME:CODE [NAME:CODE ...]\n", argv[0]);
		return 2;
	}
	need = atol(argv[1]) * 1000;
	for (i = 2; i < argc && n < MAXKEYS; i++, n++) {
		char *c = strrchr(argv[i], ':');

		if (!c)
			return 2;
		*c = 0;
		code[n] = atoi(c + 1);
		fd[n] = open_by_name(argv[i]);
		if (fd[n] < 0) {
			fprintf(stderr, "keyhold: no input device '%s'\n", argv[i]);
			return 1;
		}
	}
	setvbuf(stdout, NULL, _IOLBF, 0);
	for (;;) {
		all = 1;
		for (i = 0; i < n; i++) {
			memset(k, 0, sizeof(k));
			if (ioctl(fd[i], EVIOCGKEY(sizeof(k)), k) < 0 ||
			    !(k[code[i] / 8] & (1 << (code[i] % 8))))
				all = 0;
		}
		if (all && since < 0) {
			since = now_ms();
			puts("hold");
		} else if (all && now_ms() - since >= need) {
			puts("done");
			return 0;
		} else if (!all && since >= 0) {
			since = -1;
			puts("release");
		}
		usleep(200000);
	}
}
