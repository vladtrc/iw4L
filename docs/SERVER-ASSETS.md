# Server asset families

Hosts select optional installed families in the private lobby's
Game Setup → Game Assets submenu, before making the lobby public. Missing
installations cannot be enabled. Installed families start enabled. Console
settings are `set sv_assets_<family> 0|1`; launch defaults use
`IW4L_ASSETS_<FAMILY>` (0 or 1). Both are generated from the installed set.
IW4 remains required. A map from a disabled family cannot be selected or loaded.

A public lobby fixes its enabled families when it is created. Return to private
before changing them. Joining requires every enabled family, and all members
load exactly that selection, including weapons, sounds and materials. Disabled
families are excluded even when installed locally; cache reuse includes the
selection. Disabling optional families lets players without those games join, provided
they have IW4 and every other enabled family.

Saved classes using disabled weapons remain visible but cannot be equipped.
The restriction applies as soon as the player joins the lobby and names the
disabled family. Other classes remain selectable, and saved classes do not block
joining. Leaving the server clears its family restrictions; saved loadouts are
preserved. Select or edit a class using enabled families before spawning.

If required game files are missing, joining is blocked with a menu notice naming
the games to install. The host can instead disable those optional families.
