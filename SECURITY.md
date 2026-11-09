# Security

residue-ui moves worktrees to the trash, and restores them, through the residue CLI it bundles. A bug that makes the app remove, restore or change anything other than what its confirmation showed, run residue with other arguments, or bundle a residue other than the release `sidecar.lock` pins is a security issue. Please report it privately. Do not open a public issue for it.

Report through GitHub private vulnerability reporting: open this repository's **Security** tab and choose **Report a vulnerability**. Include the two versions at the bottom of the app's sidebar, your macOS version, and the steps that reproduce it.

A bug in what residue itself decides is safe to remove belongs to residue-cli: report it as [its SECURITY.md](https://github.com/whiteheap/residue-cli/blob/main/SECURITY.md) describes.

The app is not a security boundary between users. It runs as you, and assumes that the other processes running as you are not trying to fool it.
