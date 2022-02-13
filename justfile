# run the game
run:
  cargo run

# whenever a scene in asset-sources changes, export it
watch-scenes:
  ls asset-sources/scenes/*.json | entr just export-scene /_

# export all scenes
export-scenes:
  for scene in asset-sources/scenes/*.json; do \
    just export-scene $scene; \
  done

# export a single scene given its file path
export-scene scene:
  jq -f asset-sources/tiled/export.jq {{scene}} \
    > assets/scenes/$(basename {{scene}})
