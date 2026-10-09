#!/bin/sh
# Hooks shipped with the plugin: `hook.sh prompt` hands images pasted into the MEGANE viewer
# to Claude; `hook.sh pretool` / `hook.sh permission` let the viewer answer questions, plan
# approvals and permission prompts.
# Looks in the install locations too, because a Claude Code session started before the
# install does not have them on PATH yet. Never fails the prompt.
for bin in \
  "$(command -v megane 2> /dev/null)" \
  "${MEGANE_INSTALL_DIR:+$MEGANE_INSTALL_DIR/megane}" \
  "${LOCALAPPDATA:+$LOCALAPPDATA/Programs/megane/megane.exe}" \
  "$HOME/.local/bin/megane" \
  "$HOME/.cargo/bin/megane" \
  "$HOME/.cargo/bin/megane.exe"; do
  if [ -n "$bin" ] && [ -f "$bin" ]; then
    exec "$bin" hook "${1:-prompt}"
  fi
done
cat > /dev/null
exit 0
