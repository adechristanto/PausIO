# PausIO native bridge

This is the canonical source for the mobile-native portions of the watch bridge.

- iOS writes the latest `WatchSettingsEnvelopeV1` with `WCSession.updateApplicationContext` and retains it until the watch sends a receipt. It uses immediate `sendMessage` only for a reachable test event or action.
- Wear OS writes the same envelope to the urgent `/pausio/settings/v1` DataItem. CapabilityClient distinguishes an installed PausIO watch app from a generic connected node; listener receipts confirm application of a revision.
- Both bridges coalesce durable latest state, retain only higher revisions on watch, and use immediate test-event acknowledgements. Acknowledgement proves PausIO's watch handler ran, never that a person physically felt a haptic.
- This plugin is linked only by the mobile hosts: Apple Watch communication is iPhone-to-watch through WatchConnectivity, and Wear OS communication is Android-phone-to-watch through the Data Layer. Desktop builds do not include a wearable bridge or relay.

The generated Tauri iOS/Android hosts are deliberately excluded from source control. `scripts/generate-mobile.sh` regenerates them and applies the idempotent watch-module patch.
