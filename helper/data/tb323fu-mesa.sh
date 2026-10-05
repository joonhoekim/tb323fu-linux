# SPDX-License-Identifier: GPL-3.0-or-later
# /etc/profile.d/tb323fu-mesa.sh -- the helper's Mesa channel for login shells: the variables of
# /var/lib/tb323fu/mesa/env.conf (empty while the channel is off). Desktop sessions read the same
# file through /etc/environment.d/60-tb323fu-mesa.conf.
if [ -r /var/lib/tb323fu/mesa/env.conf ]; then
	set -a
	. /var/lib/tb323fu/mesa/env.conf
	set +a
fi
