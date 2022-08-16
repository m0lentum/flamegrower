# run the game
run:
  cargo run

# export all assets
export-all:
  just export-scenes
  just export-models

# whenever a scene in asset-sources changes, export it
watch-scenes:
  ls asset-sources/scenes/*.json | entr just export-scene /_

# export all scenes
export-scenes:
  for scene in asset-sources/scenes/*.json; do \
    just export-scene "$scene"; \
  done

# export a single scene given its file path
export-scene scene:
  jq -f asset-sources/tiled/export.jq {{scene}} \
    > assets/scenes/$(basename {{scene}})

# whenever a model in asset-sources changes, export it
watch-models:
  ls asset-sources/models/*.blend | entr just export-model /_

# export all models
export-models:
  for model in asset-sources/models/*.blend; do \
    just export-model "$model"; \
  done

# export a single model given its file path
export-model model:
  blender --background --python ./asset-sources/blender-export.py \
    "{{model}}" -- "./assets/models/$(basename {{model}} .blend)"
