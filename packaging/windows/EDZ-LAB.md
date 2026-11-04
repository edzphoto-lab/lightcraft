# LightCraft EdZ Lab — Windows x64

Personal build for Intel/AMD Windows 64-bit computers. The installer and portable ZIP are attached to the successful **EdZ Windows x64 installer** Actions run.

Download the artifact, extract its ZIP, and open `lightcraft-edz-lab-0.17.2-windows-x64.msi`. Choose **Only for me** for installation without administrator access. On managed work computers, follow your organisation's installation policy.

The application has a red lynx icon and is installed as `LightCraft EdZ Lab_0.17.2` in its own versioned directory. Each release has a distinct MSI upgrade identity and registry keys so older versions are retained. The application catalogue/preferences are still shared between versions; this is not catalogue isolation.

External editor TIFF copies retain the original's album memberships. On Windows, select external editors' `.exe` files manually in Preferences > External editors. Automatic editor discovery currently supports macOS only. Affinity and Nik must be installed separately on Windows.

This personal package is unsigned. Automated checks cover formatting, codec static analysis, codec round trips, engine export tests, repository checks, x64 PE headers, installer policy, per-user installation, executable metadata, versioned Start menu shortcut, CLI version, headless photo rendering and uninstallation. Actual laptop/GPU behaviour and third-party editor round trips still require testing on the user's PC.

## Windows validation history

The initial run `41844620640` passed formatting, workspace static analysis and HEIC/HEIF checks. The engine run passed 1,442 tests, failed the existing `sixteen_bit_zip_tiff_is_never_larger_than_uncompressed` assertion, and ignored six tests. Tests requiring private RAW fixtures, models or hardware GPU adapters could not exercise those resources on the hosted runner. The failed assertion exposed ZIP expansion of noisy 16-bit samples (1,214,459 bytes versus 1,180,486 uncompressed).

The encoder now chooses the smaller lossless representation for 16-bit ZIP exports. A synthetic incompressible RGBA regression checks exact 16-bit sample recovery, alpha, ICC and XMP. The existing photo-like noisy export regression is retained unchanged. The following build reruns the affected codec/export suites and the packaging checks rather than repeating the already-passed entire workspace static analysis.

Downloads use the native Actions upload step; ordinary shell commands do not receive the runtime artifact token. The fork-specific packaging workflow is isolated from the suite's official release jobs.
