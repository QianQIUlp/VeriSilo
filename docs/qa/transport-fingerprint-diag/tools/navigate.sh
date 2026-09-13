#!/usr/bin/env bash
# navigate.sh <silo> <url> — page goto with cert-exception click-through.
# The managed browser shows its own cert error page (net-error-card shadow
# DOM) for the self-signed diagnostic cert: expand Advanced, then accept the
# exception through the page's shadow DOM.
set -u
ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
CLI="$ROOT/apps/desktop/src-tauri/target/debug/verisilo-cli.exe"
VAULT=qa-transport-fingerprint-d96e42
SILO="$1"
URL="$2"

"$CLI" --vault "$VAULT" page "$SILO" goto "$URL" || true
sleep 1
echo "(function(){const c=document.querySelector('net-error-card'); if(!c||!c.shadowRoot) return 'no-error-page'; const sr=c.shadowRoot; const adv=sr.querySelector('#advanced-button'); if(adv&&!adv.disabled) adv.click(); return 'advanced-clicked';})()" \
  | "$CLI" --vault "$VAULT" page "$SILO" evaluate || true
sleep 1
echo "(function(){const c=document.querySelector('net-error-card'); if(!c||!c.shadowRoot) return 'no-error-page'; const sr=c.shadowRoot; const btn=sr.querySelector('#exception-button'); if(!btn) return 'no-button'; btn.disabled=false; const inner=btn.shadowRoot?btn.shadowRoot.querySelector('button'):btn; if(inner&&inner!==btn) inner.disabled=false; (inner||btn).click(); return 'clicked';})()" \
  | "$CLI" --vault "$VAULT" page "$SILO" evaluate || true
sleep 2
"$CLI" --vault "$VAULT" page "$SILO" snapshot 2>/dev/null | head -3
