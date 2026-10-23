# Sourced by compress.sh and serve.sh with $web set: web/assets, a link to the converted assets (OPENKUMITE_ASSETS when set,
# a relative one from the caller's directory; else ../assets), made again on each run. A real folder there is left as
# it is.
if [ ! -e "$web/assets" ] || [ -L "$web/assets" ]; then
  if [ -n "$OPENKUMITE_ASSETS" ]; then
    to=$(cd "$OPENKUMITE_ASSETS" && pwd) || { echo "OPENKUMITE_ASSETS: no folder $OPENKUMITE_ASSETS" >&2; exit 1; }
  else
    to=../assets
  fi
  ln -sfn "$to" "$web/assets"
fi
