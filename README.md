# unreal-shimloader

## What is this?

unreal-shimloader is a shim for RE-UE4SS that enables profile-oriented management of mods for Thunderstore Mod Manager and r2modmanPlus. It does so by virtualizing the UE4SS binary and its filesystem (and Unreal Engine's too) such that paks, mods, configs, etc. can be installed outside of the game directory.

### A note about Windows Defender

Windows Defender tends to flag shimloader binaries as malware. This is a known issue and a neverending battle. It is, obviously, a false positive.

### Not using Thunderstore Mod Manager or r2modmanPlus?

unreal-shimloader exists to give a mod manager a hook for profile-based mod management. Outside of that use case it doesn't add anything for you - it relies on `--mod-dir`/`--pak-dir`/`--cfg-dir` command-line arguments that the mod manager passes on launch, and wiring those up by hand is more work than installing UE4SS the normal way.

If you're installing manually, follow the upstream RE-UE4SS guide: <https://docs.ue4ss.com/dev/installation-guide.html>

### Command-line arguments

unreal-shimloader is controlled via the following arguments:

`--mod-dir`: The path of a directory which contains one or more directories, of which contain ue4ss lua and dll mods.
- Maps to `GAME/Binaries/Win64/Mods`

`--pak-dir`: The path of a directory which contains one or more blueprint mods (`.pak` files).
- Maps to `GAME/Content/Paks/LogicMods`

`--cfg-dir`: The path of a directory which contains config files, usually with the `.cfg` extension.
- Maps to `GAME/Config`

`--overlay-dir`: The path of a directory containing one or more wrapper-package subdirectories. Each wrapper's file tree is overlaid onto `GAME/Binaries/Win64/`.
- Maps to `GAME/Binaries/Win64`

### Thanks

A special thanks goes out to modestimpala who has contributed greatly to improving this project.
