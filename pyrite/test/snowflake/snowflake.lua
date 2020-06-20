local ice = material.refractive {ior = 1.30144, dispersion = 0.00287, color = 1}

local background = material.diffuse {
    color = spectrum {{400, 0.025}, {600, 0.0175}, {700, 0.01}} * 0.2,
}

return {
    image = {width = 512, height = 512},

    renderer = renderer.bidirectional {
        pixel_samples = 100,
        spectrum_samples = 5,
        spectrum_bins = 50,
        tile_size = 32,
        bounces = 256,
        light_samples = 2,
    },

    camera = camera.perspective {
        fov = 11,
        transform = transform.look_at {
            from = vector(15, -10, 40),
            to = vector(),
            up = vector(0, 1, 2),
        },
        focus_distance = 43.874821937,
        aperture = 3,
    },

    world = {
        sky = spectrum {{400, 0.3}, {600, 0.2}, {700, 0.1}},

        objects = {
            shape.mesh {file = "snowflake.obj", materials = {snowflake = ice}},

            shape.sphere {
                position = vector(0, 150, 50),
                radius = 30,
                material = material.emission {color = light_source.d65 * 6},
            },
            shape.sphere {
                position = vector(100, -100, 50),
                radius = 10,
                material = material.emission {color = light_source.d65 * 3},
            },
            shape.sphere {
                radius = 200,
                position = vector(0, 0, -205),
                material = background,
            },
        },
    },
}
