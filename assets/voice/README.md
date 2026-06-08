# Starfall I Voice Assets

Discussion scripts in `src/discussion.rs` reference MP3 files under this
folder. Keep recorded settlement lines at the scripted paths, for example:

- `assets/voice/settlements/cloudrail_city_01.mp3`
- `assets/voice/settlements/riftglass_village_01.mp3`
- `assets/voice/settlements/starfell_outpost_01.mp3`

Missing files are acceptable while prototyping; Bevy will load and play the MP3
when the matching dialogue line appears once the asset exists.
