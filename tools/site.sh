#!/bin/sh
# Builds the site into _site/: the documentation at the root (site/ and docs/*.md, rendered by
# tools/md.py), and the game in play/: the page (web/), the engine as WebAssembly, and every
# ruleset in games/ with a files.json listing its files.
#   sh tools/site.sh            (then: python3 -m http.server -d _site 8000)
set -e
cd "$(dirname "$0")/.."
cargo build --profile wasm --target wasm32-unknown-unknown -p openinn-web
rm -rf _site
mkdir -p _site/play/games

# The game.
cp -r web/. _site/play/
cp target/wasm32-unknown-unknown/wasm/openinn_web.wasm _site/play/openinn.wasm
for g in games/*/; do
  id=$(basename "$g")
  mkdir -p "_site/play/games/$id"
  cp "$g"*.inn "$g"*.gbx "$g"*.idle "$g"idle.toml "$g"params.toml "_site/play/games/$id/" 2>/dev/null || true
  cp "$g"*.html "$g"*.css "_site/play/games/$id/" 2>/dev/null || true   # help and theme
  (cd "_site/play/games/$id" && ls *.gbx *.inn *.idle idle.toml params.toml 2>/dev/null | sort | awk 'BEGIN{printf "["} {printf "%s\"%s\"", (NR>1?",":""), $0} END{print "]"}' > files.json)
done

# The documentation.
cp site/index.html site/docs.css _site/; cp site/*.jpg _site/ 2>/dev/null || true
python3 tools/md.py docs/RULES.md site/page.html rules > _site/rules.html
python3 tools/md.py docs/ENGINE.md site/page.html engine > _site/engine.html
examples=$(mktemp)
{
  echo "# Examples"
  echo
  echo "The rulesets in [games/](https://github.com/sirmmo/openinn/tree/main/games), complete. Copy"
  echo "one as the start of your own inn; [Rulesets](RULES.md) describes every block."
  for g in games/*/; do
    id=$(basename "$g")
    name=$(awk '/^inn /{f=1} f && /^[ \t]+name /{sub(/^[ \t]+name /, ""); print; exit}' "$g"*.inn)
    echo
    echo "## ${name:-$id}"
    echo
    echo "\`games/$id/\`: [play it](play/?game=$id)."
    for f in "$g"*.inn "$g"*.gbx "$g"idle.toml "$g"params.toml "$g"*.idle; do
      [ -f "$f" ] || continue
      echo
      echo "### $(basename "$f")"
      echo
      echo '```text'
      cat "$f"
      echo '```'
    done
  done
} > "$examples"
python3 tools/md.py "$examples" site/page.html source > _site/rulesets.html
rm -f "$examples"

touch _site/.nojekyll
echo "site built in _site/ ($(du -sh _site | cut -f1))"
