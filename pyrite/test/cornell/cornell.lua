local colors = require "colors"
local lamp = require "lamp"

local light = {
    surface = material.emissive {color = lamp.color * 3} +
        material.diffuse {color = 0.78},
}

local white = {surface = material.diffuse {color = colors.white}}

local green = {surface = material.diffuse {color = colors.green}}

local red = {surface = material.diffuse {color = colors.red}}

return {
    image = {width = 512, height = 512, white = blackbody(4000)},

    renderer = renderer.bidirectional {
        pixel_samples = 600,
        spectrum_samples = 10,
        spectrum_bins = 50,
        tile_size = 32,
        light_samples = 1,
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
                    iterations = 25,
                    threshold = 4,
                    constant = vector(-0.2, 0.8, 0, 0),
                    slice_plane = 0,
                    variant = quaternion_julia.cubic,
                },

                bounds = bounds.box {
                    min = vector(-7, -1, 0),
                    max = vector(-1, 2, 2),
                },

                material = {
                    surface = mix(
                        material.mirror {color = 1},
                            material.diffuse {color = 0.8}, fresnel(1.5)
                    ),
                },
            },
        },
    },
}
