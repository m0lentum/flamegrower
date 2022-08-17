"""Script for automatically exporting Blender models
from the command line with standard settings used in Flamegrower."""

import bpy
import sys
from bpy.app.handlers import persistent

@persistent
def export(*_):
    try:
        target_file = sys.argv[sys.argv.index("--") + 1]
    except Exception:
        print("Give the target file after a '--' on the command line")
        return

    bpy.ops.export_scene.gltf(
        filepath=target_file,
        check_existing=False,
        export_format="GLB",
        # export visible animations with vertex colors
        use_visible=True,
        export_colors=True,
        export_skins=True,
        export_animations=True,
        export_def_bones=True, # exclude control-only bones
        export_morph=True,
        optimize_animation_size=True,
        export_yup=True,
        # everything else we don't need
        export_image_format="NONE",
        export_texcoords=False,
        export_normals=False,
        export_tangents=False,
        export_morph_normal=False,
        export_morph_tangent=False,
        export_materials="NONE",
    )

bpy.app.handlers.load_post.append(export)
