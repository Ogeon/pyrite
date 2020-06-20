local light_ball = shape.sphere {
    material = material.emission {color = light_source.d65 * 20},
    position = vector(0, 0, 0),
    radius = 1,
}

return {
    image = {width = 1024, height = 512},

    renderer = renderer.simple {
        pixel_samples = 200,
        spectrum_samples = 10,
        spectrum_bins = 50,
        tile_size = 32,
        bounces = 8,
        light_samples = 4,
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
                material = material.diffuse {color = texture("tiles/color.jpg")},
                texture_scale = 5,
            },
            light_ball:with{position = vector(-1, 12, 2), radius = 3},
            light_ball:with{position = vector(15, 3, 4)},
            shape.mesh {
                file = "color_checker.obj",
                materials = {
                    color_checker = material.diffuse {
                        color = texture("color_checker.jpg"),
                    },
                },
            },
        },
    },
}
