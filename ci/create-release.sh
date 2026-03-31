#!/bin/bash
set -x
name="$1"

notes=$(cat <<EOT
See the repository changelog and release checklist for current Wezmux release notes:

- https://github.com/vcabeli/wezmux/blob/main/docs/changelog.md
- https://github.com/vcabeli/wezmux/blob/main/TODO/v1-release.md

If you're looking for installation instructions:

[README](https://github.com/vcabeli/wezmux#readme)
[Installation](https://github.com/vcabeli/wezmux/blob/main/docs/installation.md)
EOT
)

if gh release view "$name" >/dev/null 2>&1; then
  exit 0
fi

if [[ "$name" == nightly* ]]; then
  gh release create --prerelease --notes "$notes" --title "$name" "$name"
else
  gh release create --notes "$notes" --title "$name" "$name"
fi
