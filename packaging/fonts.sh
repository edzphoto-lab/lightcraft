# shellcheck shell=bash
# Font setup for local packaging; sourced after packaging/env.sh.
prepare_fonts() {
  local font_dir="${CRAFT_FONTS_DIR:-$ROOT/../craft-fonts}"
  # Relative overrides are documented relative to the workspace, regardless of the caller's cwd.
  case "$font_dir" in
    /*) ;;
    *) font_dir="$ROOT/$font_dir" ;;
  esac

  if [[ -z "${CRAFT_FONTS_DIR:-}" ]] && [[ ! -e "$font_dir" ]]; then
    # Same revision as release.yml. Never update or reset an existing checkout.
    local revision=b076417b5880bd4154bd22f00695b1d28c6bea83
    echo "==> downloading craft-fonts ($revision) to $font_dir"
    if ! git clone --no-checkout https://github.com/storytold/craft-fonts "$font_dir" ||
       ! git -C "$font_dir" checkout --detach "$revision"; then
      echo "error: could not prepare craft-fonts; provide a complete checkout with CRAFT_FONTS_DIR" >&2
      return 1
    fi
  fi

  if [[ ! -f "$font_dir/fonts/manifest.txt" ]]; then
    echo "error: missing $font_dir/fonts/manifest.txt; set CRAFT_FONTS_DIR to a complete craft-fonts checkout" >&2
    return 1
  fi
  prepare_handwriting_font "$font_dir" || return 1
  CRAFT_FONTS_DIR="$(cd "$font_dir" && pwd)" || return 1
  export CRAFT_FONTS_DIR
  # Fail the build if required faces are missing, rather than ship empty glyph boxes.
  export CRAFT_FONTS_REQUIRED=1
  echo "==> embedding fonts from $CRAFT_FONTS_DIR"
}

# Keep the contact sheet's handwritten face in the external font build input. Its unmodified
# OFL licence is copied into each package by copy_font_licences in env.sh.
prepare_handwriting_font() {
  local font_dir="$1" revision=7af0d90b4d532ffa4b041937b560c6df4e7b1e95
  local font_hash=2f6576601db015d4f6c08678120277fc8510b98c06e932ce7a6a9cbff4cbdded
  local licence_hash=f62ef357d3a1c3d27edd35a6e1ba350e8a8d13499797964eeadefbf0b3b15d1f
  local target="$font_dir/fonts/kalam" staging name expected entry
  local source="https://raw.githubusercontent.com/google/fonts/$revision/ofl/kalam"
  if [[ ! -f "$target/Kalam-Bold.ttf" ]] || [[ ! -f "$target/OFL.txt" ]] ||
     [[ "$(sha256 "$target/Kalam-Bold.ttf")" != "$font_hash" ]] ||
     [[ "$(sha256 "$target/OFL.txt")" != "$licence_hash" ]]; then
    staging="$(mktemp -d "$font_dir/.kalam.XXXXXXXX")" || return 1
    for name in Kalam-Bold.ttf OFL.txt; do
      if ! curl --fail --location --silent --show-error --retry 3 "$source/$name" -o "$staging/$name"; then
        rm -rf "$staging"
        echo "error: could not download the contact sheet handwriting font or its licence" >&2
        return 1
      fi
      expected="$font_hash"
      [[ "$name" = OFL.txt ]] && expected="$licence_hash"
      if [[ "$(sha256 "$staging/$name")" != "$expected" ]]; then
        rm -rf "$staging"
        echo "error: checksum mismatch for Kalam $name" >&2
        return 1
      fi
    done
    mkdir -p "$target" &&
      mv "$staging/Kalam-Bold.ttf" "$target/Kalam-Bold.ttf" &&
      mv "$staging/OFL.txt" "$target/OFL.txt" || { rm -rf "$staging"; return 1; }
    rmdir "$staging" || return 1
  fi
  entry="Kalam | Bold | fonts/kalam/Kalam-Bold.ttf | Latn | OFL-1.1 | fonts/kalam/OFL.txt | $font_hash | https://github.com/google/fonts/tree/$revision/ofl/kalam"
  if ! grep -Fxq "$entry" "$font_dir/fonts/manifest.txt"; then
    printf '\n%s\n' "$entry" >> "$font_dir/fonts/manifest.txt" || return 1
  fi
}
