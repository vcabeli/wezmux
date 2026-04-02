#!/bin/bash
set -x
set -e

TARGET_DIR=${1:-target}

TAG_NAME=${TAG_NAME:-$(git -c "core.abbrev=8" show -s "--format=%cd-%h" "--date=format:%Y%m%d-%H%M%S")}

HERE=$(pwd)

zipdir=Wezmux-macos-$TAG_NAME
if [[ "$BUILD_REASON" == "Schedule" ]] ; then
  zipname=Wezmux-macos-nightly.zip
else
  zipname=$zipdir.zip
fi
rm -rf $zipdir $zipname
mkdir $zipdir
cp -r assets/macos/WezTerm.app $zipdir/
# Omit MetalANGLE for now; it's a bit laggy compared to CGL,
# and on M1/Big Sur, CGL is implemented in terms of Metal anyway
mv $zipdir/WezTerm.app $zipdir/Wezmux.app
rm $zipdir/Wezmux.app/*.dylib
mkdir -p $zipdir/Wezmux.app/Contents/MacOS
mkdir -p $zipdir/Wezmux.app/Contents/Resources
cp -r assets/shell-integration/* $zipdir/Wezmux.app/Contents/Resources
cp -r assets/shell-completion $zipdir/Wezmux.app/Contents/Resources
tic -xe wezterm -o $zipdir/Wezmux.app/Contents/Resources/terminfo termwiz/data/wezterm.terminfo

for bin in wezterm wezterm-mux-server wezterm-gui strip-ansi-escapes ; do
  # If the user ran a simple `cargo build --release`, then we want to allow
  # a single-arch package to be built
  if [[ -f $TARGET_DIR/release/$bin ]] ; then
    cp $TARGET_DIR/release/$bin $zipdir/Wezmux.app/Contents/MacOS/$bin
  else
    # The CI runs `cargo build --target XXX --release` which means that
    # the binaries will be deployed in `$TARGET_DIR/XXX/release` instead of
    # the plain path above.
    # In that situation, we have two architectures to assemble into a
    # Universal ("fat") binary, so we use the `lipo` tool for that.
    lipo $TARGET_DIR/*/release/$bin -output $zipdir/Wezmux.app/Contents/MacOS/$bin -create
  fi
done

set +x
if [ -n "$MACOS_TEAM_ID" ] ; then
  MACOS_PW=$(echo $MACOS_CERT_PW | base64 --decode)
  echo "pw sha"
  echo $MACOS_PW | shasum

  # Remove pesky additional quotes from default-keychain output
  def_keychain=$(eval echo $(security default-keychain -d user))
  echo "Default keychain is $def_keychain"
  echo "Speculative delete of build.keychain"
  security delete-keychain build.keychain || true
  echo "Create build.keychain"
  security create-keychain -p "$MACOS_PW" build.keychain
  echo "Make build.keychain the default"
  security default-keychain -d user -s build.keychain
  echo "Unlock build.keychain"
  security unlock-keychain -p "$MACOS_PW" build.keychain
  echo "Import .p12 data"
  echo $MACOS_CERT | base64 --decode > /tmp/certificate.p12
  echo "decoded sha"
  shasum /tmp/certificate.p12
  security import /tmp/certificate.p12 -k build.keychain -P "$MACOS_PW" -T /usr/bin/codesign
  rm /tmp/certificate.p12
  echo "Grant apple tools access to build.keychain"
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$MACOS_PW" build.keychain
  echo "Codesign"
  /usr/bin/codesign --keychain build.keychain --force --options runtime \
    --entitlements ci/macos-entitlement.plist --deep --sign "$MACOS_TEAM_ID" $zipdir/Wezmux.app/
  echo "Restore default keychain"
  security default-keychain -d user -s $def_keychain
  echo "Remove build.keychain"
  security delete-keychain build.keychain || true
fi

set -x
zip -r $zipname $zipdir
set +x

if [ -n "$MACOS_TEAM_ID" ] ; then
  echo "Notarize"
  xcrun notarytool submit $zipname --wait --team-id "$MACOS_TEAM_ID" --apple-id "$MACOS_APPLEID" --password "$MACOS_APP_PW"
fi
set -x

SHA256=$(shasum -a 256 $zipname | cut -d' ' -f1)
sed -e "s/@TAG@/$TAG_NAME/g" -e "s/@SHA256@/$SHA256/g" < ci/wezterm-homebrew-macos.rb.template > wezmux.rb
