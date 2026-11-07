#!/bin/sh
# Locks down a server the desktop app just set up, once the app's SSH key
# works on it. Each step prints `did: <what>` when it's done and is skipped
# when it can't be done here. Nothing in it opens a port: Squad Code listens
# only on the server's own 127.0.0.1 and is reached through SSH.
#
# - SSH takes keys only: no passwords, and root only with a key.
# - A firewall lets in SSH and nothing else.
# - fail2ban blocks addresses that keep guessing at SSH.
# - Security updates install themselves.
# - Squad's own folder is readable only by you.
set -u

say() { printf '%s\n' "$*"; }

chmod 700 "$HOME/.squadcode" 2>/dev/null && say "did: kept Squad's folder private to you"

if [ "$(id -u)" = 0 ]; then
  R=""
elif sudo -n true 2>/dev/null; then
  R="sudo -n"
else
  say "skipped: the rest needs root or sudo without a password"
  exit 0
fi

# The ports sshd listens on, so the firewall never shuts us out.
ports=$($R sshd -T 2>/dev/null | awk '/^port /{print $2}')
[ -n "$ports" ] || ports=22

# Key-only SSH. Our key is in use right now, so turning passwords off can't
# lock the app out; a config that fails `sshd -t` is taken back.
if [ -d /etc/ssh ]; then
  $R mkdir -p /etc/ssh/sshd_config.d
  printf 'PasswordAuthentication no\nKbdInteractiveAuthentication no\nPermitRootLogin prohibit-password\n' \
    | $R tee /etc/ssh/sshd_config.d/10-squad.conf >/dev/null
  if grep -qE '^\s*Include\s+/etc/ssh/sshd_config.d' /etc/ssh/sshd_config 2>/dev/null && $R sshd -t 2>/dev/null; then
    ($R systemctl reload ssh 2>/dev/null || $R systemctl reload sshd 2>/dev/null || $R service ssh reload 2>/dev/null) \
      && say "did: turned off password logins for SSH"
  else
    $R rm -f /etc/ssh/sshd_config.d/10-squad.conf
    say "skipped: this server's SSH setup doesn't read sshd_config.d"
  fi
fi

if command -v apt-get >/dev/null 2>&1; then
  export DEBIAN_FRONTEND=noninteractive
  $R apt-get update -qq >/dev/null 2>&1
  $R apt-get install -y -qq ufw fail2ban unattended-upgrades >/dev/null 2>&1
fi

if command -v ufw >/dev/null 2>&1; then
  for p in $ports; do $R ufw allow "$p/tcp" >/dev/null 2>&1; done
  $R ufw default deny incoming >/dev/null 2>&1
  $R ufw default allow outgoing >/dev/null 2>&1
  $R ufw --force enable >/dev/null 2>&1 && say "did: turned on a firewall that lets in only SSH"
fi

if command -v fail2ban-client >/dev/null 2>&1; then
  $R systemctl enable --now fail2ban >/dev/null 2>&1 && say "did: turned on fail2ban against password guessing"
fi

if [ -d /etc/apt/apt.conf.d ] && command -v unattended-upgrade >/dev/null 2>&1; then
  printf 'APT::Periodic::Update-Package-Lists "1";\nAPT::Periodic::Unattended-Upgrade "1";\n' \
    | $R tee /etc/apt/apt.conf.d/20auto-upgrades >/dev/null && say "did: set security updates to install themselves"
fi
exit 0
