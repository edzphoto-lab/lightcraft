# Named external editors — Affinity Photo and Nik

This fork adds several named editors to **Photo → Edit In** and the photo's
right-click **Edit In** menu. It uses LightCraft's existing rendered-copy,
stacking and focus-return reload pipeline. It is not a Nik plug-in host.

## Configure

1. Open **Settings → External editors**.
2. Under **Named external editors**, click **Add editor**.
3. Give the entry a name, for example **Affinity Photo 2**, **Silver Efex** or
   **Color Efex**.
4. In **Application**, enter the exact macOS application name or its full
   `.app` path. Use the installed standalone Nik application, not a plug-in
   file. Actual bundle names and locations vary by Nik generation.
5. Keep **TIFF**, **16-bit**, and select the working colour space you want.
   The default is Adobe RGB; ProPhoto, Display P3 and sRGB are also offered.
6. Repeat for each editor. Preferences are saved in the app's existing settings
   file. The legacy Other Editor and PhotoCraft settings are preserved.

An entry with no application is disabled in the menu. Removing an entry removes
only its preference, never photographs or applications. Up to 256 entries can be
configured. Names are labels, not commands or shell expressions.

## Automatic discovery (macOS)

Click **Automatically detect installed editors** in the dedicated preferences tab.
A background worker searches Spotlight's indexed application bundles, `/Applications`,
`/System/Applications`, `~/Applications` and system/user `Library/Application Support`.
It includes nested Nik suite folders, reads XML/binary `Info.plist` files with macOS
`plutil`, and never launches the discovered applications.

Recognition combines known photo editor names (including Google Nik 1.2.11) with
bundle declarations for photography apps that edit photo documents. This is a
heuristic, not a guarantee of finding every photo application or of TIFF round-trip
compatibility. Non-indexed custom locations can require manual addition. Symbolic
links and bundle internals are not traversed. Folder depth and entry counts are
bounded; the result reports unreadable locations and limits reached.

Detection adds missing entries with 16-bit TIFF / Adobe RGB defaults. Existing paths,
stable ids and per-editor preferences are preserved; repeating detection does not
duplicate the same canonical application path. Different installed versions may have
separate entries. A 256-editor safety limit applies; an invalid entry or a
full list leaves preferences unchanged and reports an error.

**Remove editor** removes only the LightCraft configuration entry, never the
application or photos. A subsequent detection can rediscover a removed application.
Manual addition and the legacy PhotoCraft/Other Editor controls remain in this tab.
Detection is currently disabled on Windows, Linux and the web.

## Edit and return

1. Select a photo and choose the named editor in **Photo → Edit In** or the
   photo's right-click **Edit In** menu.
2. LightCraft renders the current corrections into a separate file, assigns a
   unique filename and adds the copy to a stack with the source photograph.
3. Edit in the chosen application and save to the **same TIFF path**.
4. Return to LightCraft. The existing window-focus handler reloads the tracked
   copies. **Photo → Reload from Disk** is also available.

For Affinity, keep a separate `.afphoto` document if you need editable layers,
but also write the finished result to the tracked TIFF. Saving only an
`.afphoto` document does not update that TIFF.

For Nik, this initial implementation requires the standalone application to
open the TIFF and save its result back to that path. A module that writes a new
filename, or only offers an export elsewhere, needs an additional integration.
Closing or cancelling the editor leaves the initial rendered copy in LightCraft;
it does not delete it automatically. If launching the application fails after
rendering, the copy is kept and the launch error is reported.

## Verification boundaries

Regression tests simulate application launches and exercise real TIFF creation,
16-bit decoding, stacking, per-editor selection/settings, reloading into the
same catalog record, stale editor ids and preference persistence. These are not
tests of Affinity or Nik itself. End-to-end checks on the user's macOS,
Affinity Photo 2 and installed Nik generation are still required. No signed or
notarized macOS installer is included in this change.

Tracking a new output filename and polling
for files saved after LightCraft has already regained focus are not added in
this first step. The existing focus-return handler remains unchanged.

## Command interface

Configuration uses `app.externalEditors`:

```json
{"action":"add","name":"Affinity Photo 2","application":"Affinity Photo 2"}
```

The result includes a stable `id`. `list` returns all entries; `update` takes
`id` plus any of `name`, `application`, `file`; `remove` takes `id`. A `file`
object can specify `format` (`tiff` / `psd`), `bitDepth` (8 / 16), and
`colorSpace` (`adobeRgb` / `proPhoto` / `displayP3` / `srgb`). The default is
16-bit TIFF in Adobe RGB.

Launch through the existing UI command:

```json
{"command":"photo.editInExternal","params":{"editorId":1}}
```

The stable id is resolved when the command runs. An entry deleted since a menu
was drawn is refused before a copy is made. No editor id means the legacy Other
Editor, preserving existing shortcuts and control clients.
