"""Generate the personal preview identity and its read-only updater pin."""
import json
import plistlib
import sys
from pathlib import Path


def prepare(template, app, version, commit):
    data = plistlib.loads(Path(template).read_bytes())
    data.update(
        CFBundleDisplayName="LightCraft EdZ Lab",
        CFBundleName="LightCraft EdZ Lab",
        CFBundleExecutable="LightCraftEdZ",
        CFBundleIdentifier="org.edzphoto.lightcraft.lab",
        CFBundleShortVersionString=version.split("-")[0],
        CFBundleVersion=version.split("-")[0],
        LightCraftVersion=version,
        LightCraftBuildCommit=commit,
        LSMinimumSystemVersion="13.0",
    )
    contents = Path(app) / "Contents"
    (contents / "Info.plist").write_bytes(plistlib.dumps(data))
    # craft-update's supported pin prevents official releases replacing the isolated app.
    (contents / "Resources" / "versions.json").write_text(
        json.dumps({"apps": {"lightcraft": {"Pinned": version}}}) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    prepare(*sys.argv[1:])
