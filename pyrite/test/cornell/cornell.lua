local colors = require "colors"
local lamp = require "lamp"

local light = material.emission {color = spectrum(lamp.color)}

local white = material.diffuse {color = spectrum(colors.white)}

local green = material.diffuse {color = spectrum(colors.green)}

local red = material.diffuse {color = spectrum(colors.red)}

return {
    image = {width = 512, height = 512},

    renderer = renderer.bidirectional {
        pixel_samples = 300,
        spectrum_samples = 10,
        spectrum_bins = 50,
        tile_size = 32,
        light_samples = 5,
        bounces = 4,
        light_bounces = 4,
    },

    camera = camera.perspective {
        fov = 37.7,
        transform = transform.look_at {
            from = vector(-2.78, -8, 2.73),
            to = vector(-2.78, 0, 2.73),
            up = vector {z = 1},
        },
    },

    world = {
        objects = {
            shape.mesh {
                file = "box.obj",
                materials = {
                    light = light,
                    left = red,
                    right = green,

                    tall = white,
                    short = white,
                    back = white,
                    ceiling = white,
                    floor = white,
                },
            },
            shape.ray_marched {
                shape = ray_marched.quaternion_julia {
                    iterations = 50,
                    threshold = 4,
                    constant = vector(-0.2, 0.8, 0, 0),
                    slice_plane = 0,
                    variant = quaternion_julia.cubic,
                },

                bounds = bounds.box {
                    min = vector(-7, -1, 0),
                    max = vector(-1, 2, 2),
                },

                material = fresnel_mix {
                    refract = material.diffuse {color = 0.8},
                    reflect = material.mirror {color = 1},
                    ior = 1.5,
                },
            },
        },
    },
}
