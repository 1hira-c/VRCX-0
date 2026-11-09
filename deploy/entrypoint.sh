#!/bin/sh
set -eu
umask 077
# Supervisor creates /data as root. Only its application subdirectory is ours.
if [ "$(id -u)" = 0 ]; then
    mkdir -p /data/profile
    chown 1000:1000 /data/profile
    chmod 0700 /data/profile
    exec setpriv --reuid=1000 --regid=1000 --init-groups --no-new-privs "$@"
fi
exec "$@"
