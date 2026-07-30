#!/usr/bin/env bash
set -euo pipefail
umask 077

readonly AGENT_VERSION='0.8.0'
readonly STATE_ROOT='/var/lib/verisilo/silos'
readonly CONFIG_PATH='/etc/verisilo/guest-agent.json'
readonly CHROMIUM='/usr/bin/chromium'
readonly BROWSER_USER='verisilo-browser'
readonly MAX_REQUEST_BYTES=16384
readonly MAX_LOG_BYTES=8192
readonly MAX_PROBE_CONFIG_BYTES=4096
readonly MAX_PROBE_RESPONSE_BYTES=4096
readonly MAX_READY_BYTES=4096
readonly EVIDENCE_VALIDITY_SECONDS=120
readonly LOOPBACK_PROXY_HOST='127.0.0.1'

fail() {
  printf '%s\n' "$1" >&2
  exit 64
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required fixed dependency is missing: $1"
}

[[ $EUID -eq 0 ]] || fail 'the fixed VeriSilo guest agent must run as root'
[[ $# -eq 3 ]] || fail 'expected: <fixed-subcommand> --silo-id <uuid>'
readonly ACTION="$1"
[[ "$2" == '--silo-id' ]] || fail 'missing --silo-id'
readonly SILO_ID="$3"
[[ "$SILO_ID" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$ ]] ||
  fail 'invalid silo UUID'

readonly SILO_ROOT="$STATE_ROOT/$SILO_ID"
readonly PROFILE_ROOT="$SILO_ROOT/chromium-profile"
readonly PID_FILE="$SILO_ROOT/chromium.pid"
readonly READY_FILE="$SILO_ROOT/network-ready.json"
readonly LOG_FILE="$SILO_ROOT/chromium.log"
readonly LOCK_FILE="$SILO_ROOT/agent.lock"
readonly GLOBAL_LOCK_FILE="$STATE_ROOT/agent.lock"
readonly ACTIVE_SILO_FILE="$STATE_ROOT/active-silo"
readonly BOUND_SILO_FILE="$STATE_ROOT/bound-silo"

require_command readlink
require_command sha256sum
require_command stat
[[ ! -L "$0" && "$(readlink -f -- "$0")" == '/opt/verisilo/bin/verisilo-guest-agent' ]] ||
  fail 'guest agent must be the fixed non-symlink path'
AGENT_OWNER="$(stat -c '%u' -- "$0")"
readonly AGENT_OWNER
AGENT_MODE="$(stat -c '%a' -- "$0")"
readonly AGENT_MODE
[[ "$AGENT_OWNER" == '0' ]] || fail 'guest agent must be root-owned'
[[ "$AGENT_MODE" == '755' ]] || fail 'guest agent mode must be exactly 0755'
AGENT_HASH_LINE="$(sha256sum -- "$0")"
readonly AGENT_HASH_LINE
AGENT_SHA256="${AGENT_HASH_LINE%% *}"
readonly AGENT_SHA256
require_command getent
BROWSER_PASSWD="$(getent passwd "$BROWSER_USER")" || fail 'dedicated browser account is missing'
readonly BROWSER_PASSWD
IFS=':' read -r BROWSER_NAME _ BROWSER_UID BROWSER_GID _ BROWSER_HOME BROWSER_SHELL <<<"$BROWSER_PASSWD"
readonly BROWSER_NAME BROWSER_UID BROWSER_GID BROWSER_HOME BROWSER_SHELL
[[ "$BROWSER_NAME" == "$BROWSER_USER" && "$BROWSER_UID" =~ ^[0-9]+$ && "$BROWSER_GID" =~ ^[0-9]+$ && "$BROWSER_HOME" == /* ]] ||
  fail 'dedicated browser account record is invalid'
((BROWSER_UID >= 1000 && BROWSER_UID < 65534 && BROWSER_GID >= 1000 && BROWSER_GID < 65534)) ||
  fail 'dedicated browser account must use an unprivileged UID and GID'
[[ "$BROWSER_SHELL" == '/usr/sbin/nologin' || "$BROWSER_SHELL" == '/bin/false' ]] ||
  fail 'dedicated browser account must have a non-login shell'

if [[ "$ACTION" == 'identity' ]]; then
  require_command jq
  jq -cn \
    --arg agent_version "$AGENT_VERSION" \
    --arg sha256 "$AGENT_SHA256" \
    --arg mode "$AGENT_MODE" \
    --arg browser_user "$BROWSER_USER" \
    --argjson browser_uid "$BROWSER_UID" \
    '{
      schemaVersion: 1,
      agentVersion: $agent_version,
      sha256: $sha256,
      ownerUid: 0,
      mode: $mode,
      path: "/opt/verisilo/bin/verisilo-guest-agent",
      browserUser: $browser_user,
      browserUid: $browser_uid
    }'
  exit 0
fi

[[ ! -L "$STATE_ROOT" && ! -L "$SILO_ROOT" ]] ||
  fail 'guest state directories must not be symbolic links'
install -d -m 0711 "$STATE_ROOT" "$SILO_ROOT"
[[ -d "$STATE_ROOT" && -d "$SILO_ROOT" && ! -L "$STATE_ROOT" && ! -L "$SILO_ROOT" ]] ||
  fail 'guest state paths must be real directories'
[[ "$(readlink -f -- "$STATE_ROOT")" == "$STATE_ROOT" && "$(readlink -f -- "$SILO_ROOT")" == "$SILO_ROOT" ]] ||
  fail 'guest state paths must not traverse symbolic-link components'
for lock_path in "$GLOBAL_LOCK_FILE" "$LOCK_FILE"; do
  if [[ -L "$lock_path" || (-e "$lock_path" && ! -f "$lock_path") ]]; then
    fail 'guest lock path must be a regular file'
  fi
done
require_command flock
exec 8>"$GLOBAL_LOCK_FILE"
flock -x 8
exec 9>"$LOCK_FILE"
flock -x 9

bind_single_silo() {
  local bound_id=''
  [[ ! -L "$BOUND_SILO_FILE" ]] || fail 'persistent Silo binding must not be a symlink'
  if [[ -f "$BOUND_SILO_FILE" ]]; then
    IFS= read -r bound_id <"$BOUND_SILO_FILE"
    [[ "$bound_id" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$ ]] ||
      fail 'persistent Silo binding is invalid'
    [[ "$bound_id" == "$SILO_ID" ]] || fail 'multi-Silo WSL profiles are gated in V0.8'
    return
  elif [[ -e "$BOUND_SILO_FILE" ]]; then
    fail 'persistent Silo binding is not a regular file'
  fi
  printf '%s\n' "$SILO_ID" >"$BOUND_SILO_FILE"
  chmod 0600 "$BOUND_SILO_FILE"
}

bind_single_silo

validate_host() {
  local host="$1"
  [[ -n "$host" && ${#host} -le 253 ]] || return 1
  [[ "$host" =~ ^[A-Za-z0-9.:\[\]-]+$ ]]
}

validate_port() {
  local port="$1"
  [[ "$port" =~ ^[0-9]+$ ]] && ((port >= 1 && port <= 65535))
}

validate_ip_address() {
  local address="$1"
  [[ -n "$address" && ${#address} -le 128 ]] || return 1
  if [[ "$address" == *.* ]]; then
    local octet
    local -a octets=()
    IFS='.' read -r -a octets <<<"$address"
    [[ ${#octets[@]} -eq 4 ]] || return 1
    for octet in "${octets[@]}"; do
      [[ "$octet" =~ ^[0-9]{1,3}$ ]] && ((10#$octet <= 255)) || return 1
    done
  else
    [[ "$address" =~ ^[0-9A-Fa-f:]+$ && "$address" == *:* ]] || return 1
  fi
}

validate_https_url() {
  local url="$1"
  [[ ${#url} -ge 9 && ${#url} -le 2048 ]] || return 1
  [[ "$url" =~ ^https://[^/@[:space:]]+(/[^[:space:]@]*)?$ ]] || return 1
  [[ "$url" != *'#'* ]]
}

https_url_hostname() {
  local url="$1" remainder authority hostname port=''
  remainder="${url#https://}"
  authority="${remainder%%/*}"
  [[ "$authority" != *'?'* && "$authority" != *'#'* && "$authority" != \[* ]] || return 1
  hostname="${authority%%:*}"
  if [[ "$authority" == *:* ]]; then
    port="${authority##*:}"
    [[ "$authority" == "$hostname:$port" ]] || return 1
    validate_port "$port" || return 1
  fi
  validate_dns_hostname "$hostname" || return 1
  printf '%s' "${hostname,,}"
}

validate_dns_hostname() {
  local hostname="$1"
  [[ -n "$hostname" && ${#hostname} -le 253 ]] || return 1
  [[ "$hostname" == *.* && "$hostname" != *..* && ! "$hostname" =~ ^[0-9.]+$ ]] || return 1
  local label
  local -a labels=()
  IFS='.' read -r -a labels <<<"$hostname"
  for label in "${labels[@]}"; do
    [[ ${#label} -ge 1 && ${#label} -le 63 ]] || return 1
    [[ "$label" =~ ^[A-Za-z0-9]([A-Za-z0-9-]*[A-Za-z0-9])?$ ]] || return 1
  done
}

read_probe_config() {
  require_command jq
  [[ -f "$CONFIG_PATH" && ! -L "$CONFIG_PATH" ]] ||
    fail 'fixed guest probe configuration is missing'
  [[ "$(readlink -f -- "$CONFIG_PATH")" == "$CONFIG_PATH" ]] ||
    fail 'guest probe configuration path must not traverse symbolic links'
  local owner mode size
  owner="$(stat -c '%u' -- "$CONFIG_PATH")"
  mode="$(stat -c '%a' -- "$CONFIG_PATH")"
  size="$(stat -c '%s' -- "$CONFIG_PATH")"
  [[ "$owner" == '0' ]] || fail 'guest probe configuration must be root-owned'
  [[ "$mode" == '600' ]] || fail 'guest probe configuration mode must be exactly 0600'
  if [[ ! "$size" =~ ^[0-9]+$ ]] || ((size < 2 || size > MAX_PROBE_CONFIG_BYTES)); then
    fail 'guest probe configuration must be at most 4 KiB'
  fi
  jq -e '
    type == "object" and
    (
      (keys | sort) == ["ipEchoUrl"] or
      (keys | sort) == ["dnsEchoUrl", "dnsProbeHostname", "expectedDnsAnswer", "ipEchoUrl"]
    ) and
    (.ipEchoUrl | type == "string" and length >= 9 and length <= 2048) and
    (
      (has("dnsEchoUrl") | not) or
      (
        (.dnsEchoUrl | type == "string" and length >= 9 and length <= 2048) and
        (.dnsProbeHostname | type == "string" and length >= 3 and length <= 253) and
        (.expectedDnsAnswer | type == "string" and length >= 2 and length <= 128)
      )
    )
  ' "$CONFIG_PATH" >/dev/null ||
    fail 'guest probe configuration has unknown or invalid fields'
  local ip_url dns_url dns_url_hostname dns_hostname expected_answer
  ip_url="$(jq -r '.ipEchoUrl' "$CONFIG_PATH")"
  validate_https_url "$ip_url" || fail 'ipEchoUrl must be a credential-free HTTPS URL without a fragment'
  https_url_hostname "$ip_url" >/dev/null ||
    fail 'ipEchoUrl must use a strict hostname so socks5h performs proxy-side name resolution'
  if jq -e 'has("dnsEchoUrl")' "$CONFIG_PATH" >/dev/null; then
    dns_url="$(jq -r '.dnsEchoUrl' "$CONFIG_PATH")"
    dns_hostname="$(jq -r '.dnsProbeHostname' "$CONFIG_PATH")"
    expected_answer="$(jq -r '.expectedDnsAnswer' "$CONFIG_PATH")"
    validate_https_url "$dns_url" || fail 'dnsEchoUrl must be a credential-free HTTPS URL without a fragment'
    [[ "$dns_url" != *'?'* ]] || fail 'dnsEchoUrl must not contain a query; the agent supplies the fixed hostname parameter'
    validate_dns_hostname "$dns_hostname" || fail 'dnsProbeHostname is not a strict DNS hostname'
    dns_url_hostname="$(https_url_hostname "$dns_url")" ||
      fail 'dnsEchoUrl must use the configured strict DNS probe hostname'
    [[ "$dns_url_hostname" == "${dns_hostname,,}" ]] ||
      fail 'dnsEchoUrl hostname must exactly match dnsProbeHostname for a proxy-side controlled-answer probe'
    validate_ip_address "$expected_answer" || fail 'expectedDnsAnswer must be one plain IPv4 or IPv6 address'
  fi
  jq -c . "$CONFIG_PATH"
}

loopback_socks5_url() {
  local port="$1"
  validate_port "$port" || fail 'loopback SOCKS5 port is invalid'
  printf 'socks5h://%s:%s' "$LOOPBACK_PROXY_HOST" "$port"
}

probe_proxy_exit() {
  local port="$1" config="$2"
  require_command curl
  local endpoint proxy response address
  endpoint="$(jq -r '.ipEchoUrl' <<<"$config")"
  proxy="$(loopback_socks5_url "$port")"
  response="$(
    curl --silent --show-error --fail \
      --connect-timeout 5 --max-time 12 \
      --proto '=https' --proto-redir '=https' --tlsv1.2 \
      --noproxy '' --proxy "$proxy" \
      --max-filesize "$MAX_PROBE_RESPONSE_BYTES" \
      "$endpoint"
  )" || return 1
  (( ${#response} <= MAX_PROBE_RESPONSE_BYTES )) || return 1
  address="$(printf '%s' "$response" | tr -d '\r\n\t ')"
  validate_ip_address "$address" || return 1
  printf '%s' "$address"
}

probe_proxy_dns() {
  local port="$1" config="$2"
  require_command curl
  if ! jq -e 'has("dnsEchoUrl")' <<<"$config" >/dev/null; then
    return 2
  fi
  local endpoint hostname expected proxy response answer
  endpoint="$(jq -r '.dnsEchoUrl' <<<"$config")"
  hostname="$(jq -r '.dnsProbeHostname' <<<"$config")"
  expected="$(jq -r '.expectedDnsAnswer' <<<"$config")"
  proxy="$(loopback_socks5_url "$port")"
  response="$(
    curl --silent --show-error --fail \
      --connect-timeout 5 --max-time 12 \
      --proto '=https' --proto-redir '=https' --tlsv1.2 \
      --noproxy '' --proxy "$proxy" \
      --max-filesize "$MAX_PROBE_RESPONSE_BYTES" \
      --get --data-urlencode "hostname=$hostname" \
      "$endpoint"
  )" || return 1
  (( ${#response} <= MAX_PROBE_RESPONSE_BYTES )) || return 1
  answer="$(printf '%s' "$response" | tr -d '\r\n\t ')"
  validate_ip_address "$answer" || return 1
  [[ "${answer,,}" == "${expected,,}" ]] || return 1
  printf '%s' "$answer"
}

atomic_write_json() {
  local destination="$1"
  local temporary
  temporary="$SILO_ROOT/.tmp-$(cat /proc/sys/kernel/random/uuid)"
  dd of="$temporary" bs=16384 count=1 status=none
  chmod 0600 "$temporary"
  mv -f -- "$temporary" "$destination"
}

process_owned_by_silo() {
  local pid="$1" silo_id="${2:-$SILO_ID}"
  local profile_root="$STATE_ROOT/$silo_id/chromium-profile"
  [[ "$pid" =~ ^[0-9]+$ && -d "/proc/$pid" ]] || return 1
  local expected actual
  expected="$(readlink -f -- "$CHROMIUM")" || return 1
  actual="$(readlink -f -- "/proc/$pid/exe")" || return 1
  [[ "$actual" == "$expected" ]] || return 1
  local key real_uid
  real_uid=''
  while read -r key real_uid _; do
    if [[ "$key" == 'Uid:' ]]; then break; fi
  done <"/proc/$pid/status"
  [[ "$real_uid" == "$BROWSER_UID" ]] || return 1
  tr '\0' '\n' <"/proc/$pid/cmdline" |
    grep -Fx -- "--user-data-dir=$profile_root" >/dev/null
}

read_pid_for_silo() {
  local silo_id="$1" pid_file="$STATE_ROOT/$1/chromium.pid"
  [[ -f "$pid_file" && ! -L "$pid_file" ]] || return 1
  local pid
  IFS= read -r pid <"$pid_file"
  process_owned_by_silo "$pid" "$silo_id" || return 1
  printf '%s' "$pid"
}

read_owned_pid() {
  read_pid_for_silo "$SILO_ID"
}

claim_active_silo() {
  local active_id=''
  [[ ! -L "$ACTIVE_SILO_FILE" ]] || fail 'global active-Silo binding must not be a symlink'
  if [[ -f "$ACTIVE_SILO_FILE" && ! -L "$ACTIVE_SILO_FILE" ]]; then
    IFS= read -r active_id <"$ACTIVE_SILO_FILE"
    if [[ ! "$active_id" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$ ]]; then
      fail 'global active-Silo binding is invalid'
    fi
    if [[ "$active_id" != "$SILO_ID" ]] && read_pid_for_silo "$active_id" >/dev/null; then
      fail 'concurrent multi-Silo Chromium is gated in V0.8'
    fi
  elif [[ -e "$ACTIVE_SILO_FILE" ]]; then
    fail 'global active-Silo binding is not a regular file'
  fi
  printf '%s\n' "$SILO_ID" >"$ACTIVE_SILO_FILE"
  chmod 0600 "$ACTIVE_SILO_FILE"
}

release_active_silo() {
  local active_id=''
  if [[ -f "$ACTIVE_SILO_FILE" && ! -L "$ACTIVE_SILO_FILE" ]]; then
    IFS= read -r active_id <"$ACTIVE_SILO_FILE"
    if [[ "$active_id" == "$SILO_ID" ]]; then rm -f -- "$ACTIVE_SILO_FILE"; fi
  fi
}

emit_action_receipt() {
  local action="$1" state="$2" retained_bytes="${3:-}"
  local observed_at
  observed_at="$(date -u +'%Y-%m-%dT%H:%M:%SZ')"
  if [[ -n "$retained_bytes" ]]; then
    jq -cn \
      --arg environment_id "$SILO_ID" \
      --arg action "$action" \
      --arg state "$state" \
      --arg agent_version "$AGENT_VERSION" \
      --arg agent_sha256 "$AGENT_SHA256" \
      --arg observed_at "$observed_at" \
      --argjson retained_browser_log_bytes "$retained_bytes" \
      '{schemaVersion: 1, environmentId: $environment_id, source: "guest_agent", action: $action, state: $state, agentVersion: $agent_version, agentSha256: $agent_sha256, observedAt: $observed_at, retainedBrowserLogBytes: $retained_browser_log_bytes}'
  else
    jq -cn \
      --arg environment_id "$SILO_ID" \
      --arg action "$action" \
      --arg state "$state" \
      --arg agent_version "$AGENT_VERSION" \
      --arg agent_sha256 "$AGENT_SHA256" \
      --arg observed_at "$observed_at" \
      '{schemaVersion: 1, environmentId: $environment_id, source: "guest_agent", action: $action, state: $state, agentVersion: $agent_version, agentSha256: $agent_sha256, observedAt: $observed_at}'
  fi
}

emit_evidence() {
  local proxy_state="$1" exit_state="$2" proxy_dns_state="$3" guest_resolver_state="$4"
  local runtime_id="$5" proxy_port="${6:-}"
  local evidence_id observed_at valid_until
  evidence_id="$(cat /proc/sys/kernel/random/uuid)"
  observed_at="$(date -u +'%Y-%m-%dT%H:%M:%SZ')"
  valid_until="$(date -u -d "+$EVIDENCE_VALIDITY_SECONDS seconds" +'%Y-%m-%dT%H:%M:%SZ')"
  jq -cn \
    --arg evidence_id "$evidence_id" \
    --arg environment_id "$SILO_ID" \
    --arg observed_at "$observed_at" \
    --arg proxy "$proxy_state" \
    --arg exit "$exit_state" \
    --arg proxy_dns "$proxy_dns_state" \
    --arg guest_resolver "$guest_resolver_state" \
    --arg runtime_id "$runtime_id" \
    --arg profile_path "$PROFILE_ROOT" \
    --arg proxy_port "$proxy_port" \
    --arg agent_version "$AGENT_VERSION" \
    --arg agent_sha256 "$AGENT_SHA256" \
    --arg valid_until "$valid_until" \
    '{
      schemaVersion: 1,
      environmentId: $environment_id,
      source: "guest_agent",
      agentVersion: $agent_version,
      agentSha256: $agent_sha256,
      observedAt: $observed_at,
      evidence: {
        schemaVersion: 1,
        evidenceId: $evidence_id,
        environmentId: $environment_id,
        source: "guest_agent",
        runtimeId: $runtime_id,
        profilePath: $profile_path,
        proxyPort: (if $proxy_port == "" then null else ($proxy_port | tonumber) end),
        agentSha256: $agent_sha256,
        proxy: $proxy,
        exit: $exit,
        proxyDns: $proxy_dns,
        guestResolver: $guest_resolver,
        observedAt: $observed_at,
        validUntil: $valid_until
      }
    }'
}

terminate_owned_browser() {
  local pid
  if ! pid="$(read_owned_pid)"; then
    rm -f -- "$PID_FILE"
    release_active_silo
    return
  fi
  kill -TERM -- "$pid"
  for _ in {1..30}; do
    if ! kill -0 -- "$pid" 2>/dev/null; then
      rm -f -- "$PID_FILE"
      release_active_silo
      return
    fi
    sleep 0.2
  done
  fail 'the exact Silo Chromium process did not stop after the explicit graceful TERM request'
}

revoke_network_authorization() {
  rm -f -- "$READY_FILE"
}

validate_ready_receipt() {
  require_command jq
  [[ -f "$READY_FILE" && ! -L "$READY_FILE" ]] || return 1
  local size
  size="$(stat -c '%s' -- "$READY_FILE")"
  [[ "$size" =~ ^[0-9]+$ ]] && ((size >= 2 && size <= MAX_READY_BYTES)) || return 1
  jq -e --arg id "$SILO_ID" --arg profile "$PROFILE_ROOT" --arg hash "$AGENT_SHA256" '
    type == "object" and
    .schemaVersion == 1 and
    .environmentId == $id and
    .profilePath == $profile and
    .agentSha256 == $hash and
    (.runtimeId | type == "string" and test("^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")) and
    (
      (.mode == "direct" and
        (keys | sort) == ["agentSha256", "environmentId", "mode", "profilePath", "proxyRequired", "runtimeId", "schemaVersion"] and
        .proxyRequired == false) or
      (.mode == "fixed_proxy" and
        (keys | sort) == ["agentSha256", "configuredAtEpoch", "environmentId", "host", "mode", "observedDnsAnswer", "observedExitAddress", "port", "profilePath", "proxyDns", "proxyRequired", "runtimeId", "schemaVersion", "scheme", "validUntilEpoch"] and
        .scheme == "socks5" and .host == "127.0.0.1" and
        (.port | type == "number" and floor == . and . >= 1 and . <= 65535) and
        (.proxyRequired | type == "boolean") and
        (.observedExitAddress | type == "string" and length >= 2 and length <= 128) and
        (
          (.proxyDns == "verified" and (.observedDnsAnswer | type == "string" and length >= 2 and length <= 128)) or
          (.proxyDns == "unavailable" and .observedDnsAnswer == null)
        ) and
        ((.proxyRequired | not) or .proxyDns == "verified") and
        (.configuredAtEpoch | type == "number" and floor == . and . > 0) and
        (.validUntilEpoch | type == "number" and floor == .) and
        (.validUntilEpoch - .configuredAtEpoch) == 120)
    )
  ' "$READY_FILE" >/dev/null || return 1

  if [[ "$(jq -r '.mode' "$READY_FILE")" == 'fixed_proxy' ]]; then
    local exit_address proxy_dns dns_answer
    exit_address="$(jq -r '.observedExitAddress' "$READY_FILE")"
    validate_ip_address "$exit_address" || return 1
    proxy_dns="$(jq -r '.proxyDns' "$READY_FILE")"
    if [[ "$proxy_dns" == 'verified' ]]; then
      dns_answer="$(jq -r '.observedDnsAnswer' "$READY_FILE")"
      validate_ip_address "$dns_answer" || return 1
    fi
  fi
}

refresh_fixed_proxy_authorization() {
  local scheme host port required config address dns_answer proxy_dns_state
  local now_epoch valid_until_epoch
  now_epoch="$(date -u +%s)"
  valid_until_epoch="$(jq -r '.validUntilEpoch' "$READY_FILE")"
  if ((now_epoch > valid_until_epoch)); then
    revoke_network_authorization
    fail 'stored proxy evidence is stale; authorization was revoked without terminating Chromium'
  fi
  scheme="$(jq -r '.scheme' "$READY_FILE")"
  host="$(jq -r '.host' "$READY_FILE")"
  port="$(jq -r '.port' "$READY_FILE")"
  required="$(jq -r '.proxyRequired' "$READY_FILE")"
  if [[ "$scheme" != 'socks5' || "$host" != "$LOOPBACK_PROXY_HOST" ]] || ! validate_port "$port"; then
    revoke_network_authorization
    fail 'stored loopback SOCKS5 binding drifted; authorization was revoked without terminating Chromium'
  fi
  config="$(read_probe_config)"
  if ! address="$(probe_proxy_exit "$port" "$config")"; then
    revoke_network_authorization
    fail 'fixed proxy exit probe failed; authorization was revoked without terminating Chromium or enabling DIRECT fallback'
  fi
  proxy_dns_state='unavailable'
  dns_answer=''
  if jq -e 'has("dnsEchoUrl")' <<<"$config" >/dev/null; then
    if dns_answer="$(probe_proxy_dns "$port" "$config")"; then
      proxy_dns_state='verified'
    else
      revoke_network_authorization
      fail 'proxy DNS probe failed or changed answer; authorization was revoked without terminating Chromium'
    fi
  fi
  if [[ "$required" == 'true' && "$proxy_dns_state" != 'verified' ]]; then
    revoke_network_authorization
    fail 'required proxy DNS configuration is unavailable; authorization was revoked without terminating Chromium'
  fi

  local refreshed_at refreshed_until
  refreshed_at="$(date -u +%s)"
  refreshed_until=$((refreshed_at + EVIDENCE_VALIDITY_SECONDS))
  jq -c \
    --arg exit_address "$address" \
    --arg proxy_dns "$proxy_dns_state" \
    --arg dns_answer "$dns_answer" \
    --argjson configured_at_epoch "$refreshed_at" \
    --argjson valid_until_epoch "$refreshed_until" \
    '.observedExitAddress = $exit_address |
     .proxyDns = $proxy_dns |
     .observedDnsAnswer = (if $dns_answer == "" then null else $dns_answer end) |
     .configuredAtEpoch = $configured_at_epoch |
     .validUntilEpoch = $valid_until_epoch' \
    "$READY_FILE" | atomic_write_json "$READY_FILE"
}

configure_network() {
  require_command jq
  if read_owned_pid >/dev/null; then
    fail 'stop the exact bound Chromium process before changing its network profile'
  fi
  # Reconfiguration is a fail-closed state transition. Once a valid stopped
  # Silo enters this operation, no prior DIRECT/proxy receipt may authorize a
  # later Start if parsing or verification below fails.
  rm -f -- "$READY_FILE"
  local payload
  payload="$(dd bs=$((MAX_REQUEST_BYTES + 1)) count=1 status=none)"
  (( ${#payload} > 0 && ${#payload} <= MAX_REQUEST_BYTES )) ||
    fail 'network request is empty or exceeds 16 KiB'
  jq -e --arg id "$SILO_ID" '
    type == "object" and
    (keys | sort) == ["environmentId", "network", "runtimeId", "schemaVersion"] and
    .schemaVersion == 1 and
    .environmentId == $id and
    (.runtimeId | type == "string" and test("^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")) and
    (.network | type == "object") and
    (
      (.network.mode == "direct" and (.network | keys) == ["mode"]) or
      (
        .network.mode == "fixed_proxy" and
        (.network | keys | sort) == ["host", "mode", "port", "proxyRequired", "scheme"] and
        (.network.proxyRequired | type == "boolean") and
        (.network.host | type == "string") and
        (.network.port | type == "number" and floor == . and . >= 1 and . <= 65535) and
        .network.scheme == "socks5" and
        (.network.host == "127.0.0.1")
      )
    )
  ' <<<"$payload" >/dev/null ||
    fail 'network request has unknown fields, a mismatched UUID, or invalid values'

  local mode runtime_id
  mode="$(jq -r '.network.mode' <<<"$payload")"
  runtime_id="$(jq -r '.runtimeId' <<<"$payload")"
  if [[ "$mode" == 'direct' ]]; then
    jq -cn \
      --arg environment_id "$SILO_ID" \
      --arg runtime_id "$runtime_id" \
      --arg profile_path "$PROFILE_ROOT" \
      --arg agent_sha256 "$AGENT_SHA256" \
      '{
        schemaVersion: 1,
        environmentId: $environment_id,
        runtimeId: $runtime_id,
        profilePath: $profile_path,
        agentSha256: $agent_sha256,
        mode: "direct",
        proxyRequired: false
      }' | atomic_write_json "$READY_FILE"
    emit_evidence 'not_requested' 'not_requested' 'not_requested' 'unavailable' "$runtime_id"
    return
  fi

  local scheme host port required
  scheme="$(jq -r '.network.scheme' <<<"$payload")"
  host="$(jq -r '.network.host' <<<"$payload")"
  port="$(jq -r '.network.port' <<<"$payload")"
  required="$(jq -r '.network.proxyRequired' <<<"$payload")"
  validate_host "$host" || fail 'proxy host contains unsupported characters'
  validate_port "$port" || fail 'proxy port is invalid'

  [[ "$scheme" == 'socks5' && "$host" == "$LOOPBACK_PROXY_HOST" ]] ||
    fail 'WSL evidence accepts only the current Silo loopback SOCKS5 endpoint'

  local config address='' dns_answer='' proxy_dns_state='unavailable'
  config="$(read_probe_config)"
  if ! address="$(probe_proxy_exit "$port" "$config")"; then
    revoke_network_authorization
    fail 'loopback SOCKS5 did not produce valid guest-origin HTTPS exit evidence; DIRECT fallback is forbidden'
  fi
  if jq -e 'has("dnsEchoUrl")' <<<"$config" >/dev/null; then
    if dns_answer="$(probe_proxy_dns "$port" "$config")"; then
      proxy_dns_state='verified'
    else
      revoke_network_authorization
      fail 'the self-hosted proxy DNS probe failed or returned the wrong expected answer'
    fi
  fi
  if [[ "$required" == 'true' && "$proxy_dns_state" != 'verified' ]]; then
    revoke_network_authorization
    fail 'required proxy mode needs configured and verified proxy DNS evidence'
  fi

  local configured_at_epoch valid_until_epoch
  configured_at_epoch="$(date -u +%s)"
  valid_until_epoch=$((configured_at_epoch + EVIDENCE_VALIDITY_SECONDS))
  jq -cn \
    --arg environment_id "$SILO_ID" \
    --arg runtime_id "$runtime_id" \
    --arg profile_path "$PROFILE_ROOT" \
    --arg agent_sha256 "$AGENT_SHA256" \
    --arg scheme "$scheme" \
    --arg host "$host" \
    --argjson port "$port" \
    --argjson required "$required" \
    --arg exit_address "$address" \
    --arg proxy_dns "$proxy_dns_state" \
    --arg dns_answer "$dns_answer" \
    --argjson configured_at_epoch "$configured_at_epoch" \
    --argjson valid_until_epoch "$valid_until_epoch" \
    '{
      schemaVersion: 1,
      environmentId: $environment_id,
      runtimeId: $runtime_id,
      profilePath: $profile_path,
      agentSha256: $agent_sha256,
      mode: "fixed_proxy",
      proxyRequired: $required,
      scheme: $scheme,
      host: $host,
      port: $port,
      observedExitAddress: $exit_address,
      proxyDns: $proxy_dns,
      observedDnsAnswer: (if $dns_answer == "" then null else $dns_answer end),
      configuredAtEpoch: $configured_at_epoch,
      validUntilEpoch: $valid_until_epoch
    }' | atomic_write_json "$READY_FILE"
  # The controlled service answer is guest-observed through socks5h. The
  # guest OS resolver is intentionally not queried and remains unavailable.
  emit_evidence 'verified' 'verified' "$proxy_dns_state" 'unavailable' "$runtime_id" "$port"
}

detach_silo() {
  require_command jq
  if read_owned_pid >/dev/null; then
    fail 'stop the bound Chromium process before detaching its profile'
  fi

  local payload
  payload="$(dd bs=$((MAX_REQUEST_BYTES + 1)) count=1 status=none)"
  (( ${#payload} > 0 && ${#payload} <= MAX_REQUEST_BYTES )) ||
    fail 'detach request is empty or exceeds 16 KiB'
  jq -e --arg id "$SILO_ID" '
    type == "object" and
    (keys | sort) == ["confirmDestroy", "environmentId", "schemaVersion"] and
    .schemaVersion == 1 and
    .environmentId == $id and
    .confirmDestroy == true
  ' <<<"$payload" >/dev/null ||
    fail 'detach requires an exact UUID-bound request with confirmDestroy=true'

  [[ "$PROFILE_ROOT" == "$STATE_ROOT/$SILO_ID/chromium-profile" ]] ||
    fail 'derived Chromium profile path is invalid'
  for artifact in "$READY_FILE" "$PID_FILE" "$LOG_FILE" "$LOCK_FILE"; do
    if [[ -L "$artifact" || (-e "$artifact" && ! -f "$artifact") ]]; then
      fail 'refusing to detach unexpected guest state'
    fi
  done

  if [[ -L "$PROFILE_ROOT" || (-e "$PROFILE_ROOT" && ! -d "$PROFILE_ROOT") ]]; then
    fail 'refusing to detach a Chromium profile that is not a real directory'
  fi
  if [[ -d "$PROFILE_ROOT" ]]; then
    rm -r --one-file-system -- "$PROFILE_ROOT"
  fi
  rm -f -- "$READY_FILE" "$PID_FILE" "$LOG_FILE" "$LOCK_FILE"
  release_active_silo
  rmdir -- "$SILO_ROOT" || fail 'unexpected guest artifacts remain after profile detach'

  local bound_id=''
  IFS= read -r bound_id <"$BOUND_SILO_FILE"
  [[ "$bound_id" == "$SILO_ID" ]] || fail 'persistent Silo binding changed during detach'
  rm -f -- "$BOUND_SILO_FILE"
  emit_action_receipt 'detach' 'destroyed'
}

start_browser() {
  require_command jq
  [[ -x "$CHROMIUM" ]] || fail 'fixed Chromium path is missing'
  local existing_pid=''
  existing_pid="$(read_owned_pid || true)"
  if [[ -z "$existing_pid" ]]; then rm -f -- "$PID_FILE"; fi
  if ! validate_ready_receipt; then
    revoke_network_authorization
    fail 'network evidence receipt is missing, oversized, or invalid; authorization was revoked without terminating Chromium'
  fi

  local -a arguments=(
    "--user-data-dir=$PROFILE_ROOT"
    '--no-first-run'
    '--no-default-browser-check'
  )
  local mode
  mode="$(jq -r '.mode' "$READY_FILE")"
  if [[ "$mode" == 'direct' ]]; then
    arguments+=('--no-proxy-server')
  else
    local scheme host port authority
    refresh_fixed_proxy_authorization
    scheme="$(jq -r '.scheme' "$READY_FILE")"
    host="$(jq -r '.host' "$READY_FILE")"
    port="$(jq -r '.port' "$READY_FILE")"
    authority="$host"
    if [[ "$host" == *:* && "$host" != \[*\] ]]; then authority="[$host]"; fi
    arguments+=("--proxy-server=$scheme://$authority:$port")
    arguments+=(
      '--proxy-bypass-list=<-loopback>'
      "--host-resolver-rules=MAP * ~NOTFOUND , EXCLUDE $host"
      '--disable-quic'
      '--webrtc-ip-handling-policy=disable_non_proxied_udp'
    )
  fi

  for profile_path in "$PROFILE_ROOT" "$PROFILE_ROOT/home" "$PROFILE_ROOT/cache" "$PROFILE_ROOT/config"; do
    if [[ -L "$profile_path" || (-e "$profile_path" && ! -d "$profile_path") ]]; then
      fail 'Chromium profile paths must be real directories'
    fi
  done
  install -d -o "$BROWSER_UID" -g "$BROWSER_GID" -m 0700 "$PROFILE_ROOT"
  install -d -o "$BROWSER_UID" -g "$BROWSER_GID" -m 0700 \
    "$PROFILE_ROOT/home" "$PROFILE_ROOT/cache" "$PROFILE_ROOT/config"
  for profile_path in "$PROFILE_ROOT" "$PROFILE_ROOT/home" "$PROFILE_ROOT/cache" "$PROFILE_ROOT/config"; do
    [[ "$(readlink -f -- "$profile_path")" == "$profile_path" ]] ||
      fail 'Chromium profile path traverses a symbolic link'
  done
  claim_active_silo

  if [[ -n "$existing_pid" ]]; then
    claim_active_silo
    emit_action_receipt 'start' 'started'
    return
  fi

  if [[ -L "$LOG_FILE" || (-e "$LOG_FILE" && ! -f "$LOG_FILE") ]]; then
    fail 'Chromium log path must be a regular file'
  fi
  : >"$LOG_FILE"
  chmod 0600 "$LOG_FILE"
  require_command setpriv
  HOME="$PROFILE_ROOT/home" \
    XDG_CACHE_HOME="$PROFILE_ROOT/cache" \
    XDG_CONFIG_HOME="$PROFILE_ROOT/config" \
    setpriv \
      --reuid "$BROWSER_UID" \
      --regid "$BROWSER_GID" \
      --clear-groups \
      --no-new-privs \
      --inh-caps=-all \
      --ambient-caps=-all \
      --bounding-set=-all \
      -- "$CHROMIUM" "${arguments[@]}" >>"$LOG_FILE" 2>&1 &
  local pid=$!
  printf '%s\n' "$pid" >"$PID_FILE"
  chmod 0600 "$PID_FILE"
  sleep 1
  if ! process_owned_by_silo "$pid"; then
    rm -f -- "$PID_FILE"
    release_active_silo
    fail 'Chromium did not remain alive with the exact Silo profile'
  fi
  emit_action_receipt 'start' 'started'
}

stop_browser() {
  terminate_owned_browser
  emit_action_receipt 'stop' 'stopped'
}

health() {
  require_command jq
  local pid
  pid="$(read_owned_pid)" ||
    fail 'recorded PID is missing or does not belong to this Silo Chromium'
  if ! validate_ready_receipt; then
    revoke_network_authorization
    fail 'network evidence receipt is missing, oversized, or invalid; authorization was revoked without terminating Chromium'
  fi
  local mode
  mode="$(jq -r '.mode' "$READY_FILE")"
  if [[ "$mode" == 'fixed_proxy' ]]; then
    refresh_fixed_proxy_authorization
  fi
  emit_action_receipt 'health' 'healthy'
}

bounded_logs() {
  require_command jq
  local bytes=0
  if [[ -L "$LOG_FILE" || (-e "$LOG_FILE" && ! -f "$LOG_FILE") ]]; then
    fail 'Chromium log path is not a regular file'
  fi
  if [[ -f "$LOG_FILE" && ! -L "$LOG_FILE" ]]; then
    bytes="$(stat -c '%s' -- "$LOG_FILE")"
    if ((bytes > MAX_LOG_BYTES)); then bytes=$MAX_LOG_BYTES; fi
  fi
  emit_action_receipt 'logs' 'logs_exported' "$bytes"
}

case "$ACTION" in
  configure-network) configure_network ;;
  start) start_browser ;;
  stop) stop_browser ;;
  detach) detach_silo ;;
  health) health ;;
  logs) bounded_logs ;;
  *) fail 'subcommand is not in the fixed allowlist' ;;
esac
