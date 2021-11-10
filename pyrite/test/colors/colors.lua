local materials = require "materials"

local ball = shape.sphere {radius = 1, position = vector {z = 1}}

return {
    image = {width = 512, height = 512},

    renderer = renderer.photon_mapping {
        pixel_samples = 500,
        spectrum_samples = 10,
        spectrum_bins = 50,
        tile_size = 32,
        bounces = 8,
        light_samples = 0,
        light_bounces = 4,
        iterations = 500,
        photons = 100000,
        initial_radius = 0.1,
    },

    camera = camera.perspective {
        fov = 53,
        transform = transform.look_at {
            from = vector {z = 30},
            to = vector(),
            up = vector {y = 1},
        },
    },

    world = {
        objects = {
            shape.sphere {
                radius = 4,
                position = vector {z = 8},
                material = materials.light,
            },

            shape.sphere {
                radius = 200,
                position = vector {z = -200},
                material = materials.floor,
            },

            ball:with{
                position = ball.position:with{x = 10},
                material = materials.glass_400nm,
            },
            ball:with{
                position = ball.position:with{x = 8.66, y = 5},
                material = materials.glass_430nm,
            },
            ball:with{
                position = ball.position:with{x = 5, y = 8.66},
                material = materials.glass_461nm,
            },
            ball:with{
                position = ball.position:with{y = 10},
                material = materials.glass_492nm,
            },
            ball:with{
                position = ball.position:with{x = -5, y = 8.66},
                material = materials.glass_523nm,
            },
            ball:with{
                position = ball.position:with{x = -8.66, y = 5},
                material = materials.glass_554nm,
            },
            ball:with{
                position = ball.position:with{x = -10},
                material = materials.glass_585nm,
            },
            ball:with{
                position = ball.position:with{x = -8.66, y = -5},
                material = materials.glass_616nm,
            },
            ball:with{
                position = ball.position:with{x = -5, y = -8.66},
                material = materials.glass_647nm,
            },
            ball:with{
                position = ball.position:with{y = -10},
                material = materials.glass_678nm,
            },
            ball:with{
                position = ball.position:with{x = 5, y = -8.66},
                material = materials.glass_709nm,
            },
            ball:with{
                position = ball.position:with{x = 8.66, y = -5},
                material = materials.glass_740nm,
            },
        },
    },
}
