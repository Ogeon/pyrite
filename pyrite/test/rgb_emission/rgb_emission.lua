local ball = shape.sphere {radius = 1, position = vector(0, 2, 0)}

local color_ball = ball:with{
    material = {surface = material.emissive {color = rgb(1, 0, 0)}},
}

return {
    image = {width = 1024, height = 256},

    renderer = renderer.simple {
        pixel_samples = 500,
        spectrum_samples = 5,
        spectrum_bins = 50,
        tile_size = 32,
        light_samples = 5,
    },

    camera = camera.perspective {
        fov = 53,
        transform = transform.look_at {
            from = vector(0, 0, 15),
            to = vector(0, 0, 0),
        },
    },

    world = {
        objects = {
            shape.plane {
                origin = vector {z = 1},
                normal = vector {z = 1},
                material = {surface = material.diffuse {color = 0.8}},
            },
            color_ball:with{
                material = {
                    surface = color_ball.material.surface:with{
                        color = rgb(1, 0, 0),
                    },
                },
                position = color_ball.position:with{x = -6.25},
            },
            color_ball:with{
                material = {
                    surface = color_ball.material.surface:with{
                        color = rgb(1, 1, 0),
                    },
                },
                position = color_ball.position:with{x = -3.75},
            },
            color_ball:with{
                material = {
                    surface = color_ball.material.surface:with{
                        color = rgb(0, 1, 0),
                    },
                },
                position = color_ball.position:with{x = -1.25},
            },
            color_ball:with{
                material = {
                    surface = color_ball.material.surface:with{
                        color = rgb(0, 1, 1),
                    },
                },
                position = color_ball.position:with{x = 1.25},
            },
            color_ball:with{
                material = {
                    surface = color_ball.material.surface:with{
                        color = rgb(0, 0, 1),
                    },
                },
                position = color_ball.position:with{x = 3.75},
            },
            color_ball:with{
                material = {
                    surface = color_ball.material.surface:with{
                        color = rgb(1, 0, 1),
                    },
                },
                position = color_ball.position:with{x = 6.25},
            },
        },
    },
}
