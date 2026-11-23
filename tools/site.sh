#!/bin/sh
# Builds the site into _site/: the page (web/), the engine as WebAssembly, and every ruleset in
# games/ with a files.json listing its files.
#   sh tools/site.sh            (then: python3 -m http.server -d _site 8000)
set -e
cd "$(dirname "$0")/.."
cargo build --profile wasm --target wasm32-unknown-unknown -p openinn-web
rm -rf _site
mkdir -p _site/games
cp -r web/. _site/
cp target/wasm32-unknown-unknown/wasm/openinn_web.wasm _site/openinn.wasm
for g in games/*/; do
  id=$(basename "$g")
  mkdir -p "_site/games/$id"
  cp "$g"*.inn "$g"*.gbx "_site/games/$id/" 2>/dev/null || true
  (cd "_site/games/$id" && ls *.gbx *.inn 2>/dev/null | sort | awk 'BEGIN{printf "["} {printf "%s\"%s\"", (NR>1?",":""), $0} END{print "]"}' > files.json)
done
touch _site/.nojekyll
echo "site built in _site/ ($(du -sh _site | cut -f1))"
