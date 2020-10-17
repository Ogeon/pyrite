local light_ball = shape.sphere {
    material = {surface = {emission = light_source.d65 * 20}},
    position = vector(0, 0, 0),
    radius = 1,
}

local floor_material = {
    surface = {
        reflection = mix(
            material.mirror {color = 1},
                material.diffuse {color = texture("tiles/color.jpg")},
                fresnel(1.5)
        ),
    },
    normal_map = texture("tiles/normal.jpg", "linear") * vector(1, -1, 1),
}

return {
    image = {width = 1024, height = 512},

    renderer = renderer.simple {
        pixel_samples = 200,
        spectrum_samples = 10,
        spectrum_bins = 50,
        tile_size = 32,
    },

    camera = camera.perspective {
        fov = 53,
        transform = transform.look_at {
            from = vector(0, 2, 12),
            to = vector(0, 2, 0),
        },
    },

    world = {
        objects = {
            shape.plane {
                origin = vector(),
                normal = vector {y = 1},
                material = floor_material,
                texture_scale = 5,
            },
            light_ball:with{position = vector(-1, 12, 2), radius = 3},
            light_ball:with{position = vector(15, 3, 4)},
            shape.mesh {
                file = "color_checker.obj",
                materials = {
                    color_checker = {
                        surface = {
                            reflection = material.diffuse {
                                color = texture("color_checker.jpg"),
                            },
                        },
                    },
                },
            },
            shape.sphere {
                position = vector(-3, 1, 0),
                radius = 1,
                texture_scale = vector(0.5, 1),
                material = {
                    surface = {
                        reflection = material.diffuse {
                            color = texture("tactile_paving/color.jpg"),
                        },
                    },
                    normal_map = texture("tactile_paving/normal.jpg", "linear") *
                        vector(1, -1, 1),
                },
            },
            shape.mesh {
                file = "cube.obj",
                transform = transform.look_at {
                    from = vector(2, 0.5, 1),
                    to = vector(-1, 0.5, 2),
                },
                materials = {
                    cube = {
                        surface = {
                            reflection = material.diffuse {
                                color = texture("fabric/color.jpg"),
                            },
                        },
                        normal_map = texture("fabric/normal.jpg", "linear") *
                            vector(1, -1, 0.1), -- artificially enhances the contrast
                    },
                },
            },
        },
    },
}
