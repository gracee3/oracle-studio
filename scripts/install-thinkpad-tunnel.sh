#!/usr/bin/env bash
set -Eeuo pipefail

if [[ $# -ne 2 ]]; then
    printf 'usage: %s SSH_TARGET ABSOLUTE_REMOTE_REPOSITORY\n' "${0##*/}" >&2
    exit 2
fi

ssh_target=$1
remote_root=${2%/}
script_dir=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repository_root=$(CDPATH='' cd -- "$script_dir/.." && pwd)
source_dir=$repository_root/tools/remote-client
config_file=$HOME/.config/oracle-studio/tunnel.env
unit_file=$HOME/.config/systemd/user/oracle-studio-tunnel.service
client_file=$HOME/.local/bin/oracle-studio-tunnel

[[ $ssh_target != -* ]] || {
    printf 'SSH target may not begin with a dash\n' >&2
    exit 2
}
[[ $remote_root = /* ]] || {
    printf 'remote repository must be an absolute path\n' >&2
    exit 2
}
[[ ${EUID:-$(id -u)} -ne 0 ]] || {
    printf 'run this installer as the ThinkPad desktop user, not root\n' >&2
    exit 1
}

install -Dm0755 "$source_dir/oracle-studio-tunnel" "$client_file"
install -Dm0644 "$source_dir/oracle-studio-tunnel.service" "$unit_file"

if [[ ! -e $config_file ]]; then
    install -Dm0600 /dev/null "$config_file"
    printf 'ORACLE_STUDIO_SSH_TARGET=%q\nORACLE_STUDIO_REMOTE_ROOT=%q\n' \
        "$ssh_target" "$remote_root" >"$config_file"
else
    printf 'Preserved existing configuration at %s\n' "$config_file"
fi

systemctl --user daemon-reload
systemctl --user enable --now oracle-studio-tunnel.service

printf 'Installed and started oracle-studio-tunnel.service\n'
printf 'Current port: oracle-studio-tunnel port\n'
printf 'Private launch URL: oracle-studio-tunnel url\n'
