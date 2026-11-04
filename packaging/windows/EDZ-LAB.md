# LightCraft EdZ Lab — Windows x64

Personal build for Intel/AMD Windows 64-bit computers. The installer and portable ZIP are attached to the successful **EdZ Windows x64 installer** Actions run.

Download the artifact, extract its ZIP, and open `lightcraft-edz-lab-0.17.2-windows-x64.msi`. Choose **Only for me** for installation without administrator access. On managed work computers, follow your organisation's installation policy.

The application has a red lynx icon and is installed as `LightCraft EdZ Lab_0.17.2` in its own versioned directory. Each release has a distinct MSI upgrade identity and registry keys so older versions are retained. The application catalogue/preferences are still shared between versions; this is not catalogue isolation.

External editor TIFF copies retain the original's album memberships. On Windows, select external editors' `.exe` files manually in Preferences > External editors. Automatic editor discovery currently supports macOS only. Affinity and Nik must be installed separately on Windows.

This personal package is unsigned. Automated checks cover the Rust gate, x64 PE headers, installer policy, per-user installation, executable metadata, versioned Start menu shortcut, CLI version, headless photo rendering and uninstallation. Actual laptop/GPU behaviour and third-party editor round trips still require testing on the user's PC.
