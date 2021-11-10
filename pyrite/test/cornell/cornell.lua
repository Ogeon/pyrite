local colors = require "colors"
local lamp = require "lamp"

local light = {
    surface = {
        reflection = material.diffuse {color = 0.78},
        emission = lamp.color * 3,
    },
}

local white = {surface = {reflection = material.diffuse {color = colors.white}}}

local green = {surface = {reflection = material.diffuse {color = colors.green}}}

local red = {surface = {reflection = material.diffuse {color = colors.red}}}

return {
    image = {width = 512, height = 512, white = blackbody(4000)},

    renderer = renderer.photon_mapping {
        pixel_samples = 200,
        spectrum_samples = 10,
        spectrum_bins = 50,
        tile_size = 32,
        bounces = 8,
        light_bounces = 4,
        iterations = 100,
        photons = 1000000,
        initial_radius = 0.02,
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
            shape.sphere {
                position = vector(-4, 1, 0.5),
                radius = 0.5,
                material = {
                    surface = {
                        reflection = material.refractive {color = 1, ior = 1.5},
                    },
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
                    surface = {
                        reflection = mix(
                            material.mirror {color = 1},
                                material.diffuse {color = 0.8}, fresnel(1.5)
                        ),
                    },
                },
            },
        },
    },
}
