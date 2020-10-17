local diamond = {
    surface = {
        reflection = material.refractive {
            ior = 2.37782,
            dispersion = 0.01371,
            color = 1,
        },
    },
}

local plexi = {
    surface = {reflection = material.mirror {color = mix(0, 0.2, fresnel(1.1))}},
}

return {
    image = {width = 512, height = 300},

    renderer = renderer.simple {
        pixel_samples = 1000,
        spectrum_samples = 1,
        spectrum_bins = 50,
        tile_size = 32,
        bounces = 256,
    },

    camera = camera.perspective {
        fov = 12.5,
        transform = transform.look_at {
            from = vector(-6.55068, -8.55076, 4.0),
            to = vector(0.1, 0, 0.1),
            up = vector {z = 1},
        },
    },

    world = {
        objects = {
            shape.mesh {
                file = "diamonds.obj",

                materials = {
                    diamonds = diamond,
                    light_left = {surface = {emission = light_source.d65}},
                    light_right = {surface = {emission = light_source.d65 * 2}},
                    bottom = plexi,
                },
            },
        },
    },
}
