# Changelog

Up to 0.0.10, the Mac app shipped in each residue-cli release, with residue's version, and its notes are in [residue-cli's CHANGELOG](https://github.com/whiteheap/residue-cli/blob/main/CHANGELOG.md).

## Unreleased

- The app has a home and a version of its own: it is built and released from this repository, and it bundles one residue-cli release, pinned in `sidecar.lock`, now 0.0.10. Every build downloads that release's two macOS archives, checks each one's sha256 and build-provenance attestation, and checks that the app's `residue-cli` is that release by its version and each architecture's UUID. The sidebar shows both versions. When the bundled residue is missing, the app says to download it again from this repository's releases.
- The app ships 25,440 B less JavaScript, 4.3 % (590,216 B to 564,776 B; 182,822 B to 177,899 B gzip). Each Phosphor icon carried its drawing in all six weights; the build now keeps the three the app draws, regular, bold and fill, and fails when the app's source or the Kumo code it bundles uses another, or one it cannot read, since that icon would render blank. Every icon and weight the app draws renders the same SVG as before. The build also fails when the bundle holds a Kumo component that `src/styles.css` does not scan, which would render unstyled, or when that list names one the bundle no longer holds (residue-cli#123, residue-cli#124).
