local ball = shape.sphere {radius = 1.5, position = vector(0, 1.4, 10)}

return {
    image = {width = 512, height = 256},

    camera = camera.perspective {
        fov = 53,
        transform = transform.look_at(
            {from = vector(0, 1, 0), to = vector(0, 1, 1)}
        ),
    },

    renderer = renderer.simple {
        pixel_samples = 300,
        spectrum_samples = 10,
        spectrum_bins = 50,
        tile_size = 32,
        light_samples = 20,
    },

    world = {
        objects = {
            shape.sphere {
                radius = 50.0,
                position = vector(0, -50, 10),
                material = {surface = material.diffuse {color = 1}},
            },

            ball:with{
                position = ball.position:with{y = 1.5},
                material = {
                    surface = material.emission {color = light_source.d65 * 3},
                },
            },

            ball:with{
                position = ball.position:with{x = -3},
                material = {
                    surface = fresnel_mix {
                        ior = 1.5,
                        reflect = material.mirror {color = 1},
                        refract = material.diffuse {
                            color = spectrum {
                                {400, 0},
                                {450, 0.3},
                                {500, 0},
                                {550, 1},
                                {600, 0},
                            },
                        },
                    },
                },
            },

            ball:with{
                position = ball.position:with{x = 3},
                material = {
                    surface = material.diffuse {
                        color = spectrum {
                            {580, 0},
                            {600, 1},
                            {610, 1},
                            {650, 0},
                        },
                    },
                },
            },
        },
    },
}
