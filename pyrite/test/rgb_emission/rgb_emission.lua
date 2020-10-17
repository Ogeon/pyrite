local ball = shape.sphere {radius = 1, position = vector(0, 2, 0)}

function color_ball(x, color)
    return ball:with{
        material = {surface = {emission = color}},
        position = ball.position:with{x = x},
    }
end

return {
    image = {width = 1024, height = 256},

    renderer = renderer.simple {
        pixel_samples = 400,
        spectrum_samples = 20,
        spectrum_bins = 50,
        tile_size = 32,
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
                material = {
                    surface = {reflection = material.diffuse {color = 0.8}},
                },
            },
            color_ball(-6.25, rgb(1, 0, 0)),
            color_ball(-3.75, rgb(1, 1, 0)),
            color_ball(-1.25, rgb(0, 1, 0)),
            color_ball(1.25, rgb(0, 1, 1)),
            color_ball(3.75, rgb(0, 0, 1)),
            color_ball(6.25, rgb(1, 0, 1)),
        },
    },
}
