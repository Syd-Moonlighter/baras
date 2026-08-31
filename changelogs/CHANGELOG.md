# v2026.8.23

## PvP

- New **Enemy Frames** overlay showing the enemy team's HP, disciplines, and current targets. Enemy players are picked up as they appear in the logs and replaced oldest-first when the roster fills
- Added **Incoming Damage** overlay showing your DTPS by source for the past 20 seconds.
- Raid frames will now clear automatically when entering a new PvP instance
- Improved encounter start/end detection for Arenas and Warzones
- Metrics overlays now split teams into separate sections

## Raid Frames

- Added a **Clear All** button to the overlay `Rearrange` mode
- Added an **Effect Horizontal Offset** setting to nudge effect icons left or right
- Improved auto-assignment text-matching algorithm
- Reverted raid frame entry text back to white

## Effect Modifiers

- New modifier triggers: **Healing Dealt**, **Killing Blow**, and **Resource Spent** (optionally scaling the duration change by the amount spent)
- Modifiers can now **cancel** an effect instead of adjusting its duration
- Modifier ICD can now start from the effect's initial application and be scaled by alacrity

## Other

- Operations timer now auto-starts in Dxun, Gods from the Machine, and R-4 Anomaly relevant difficulties when log events are recorded past the banners
- Combat log tooltips in the data explorer now show HP
- Renamed "Prioritize Stacked Effects" to "Emphasize Effect Charges"
- Improved death recap formatting
- Icons are now slightly smaller relative to bar height in effect overlays

## Bugfixes

- Hotkey input elements now correctly captures key codes instead of output. Numpad keys can now be assigned as hotkeys.
- Reduced frequency of final boss HP incorrectly displaying as 100% after wipes
- Fixed issue with Windows taskbar appearing in OCR screen capture
- Fixed issue with alacrity effect application windows expiring at incorrect time
