return {
    image = {width = 512, height = 300},

    renderer = renderer.bidirectional {
        pixel_samples = 100,
        spectrum_samples = 10,
        spectrum_bins = 50,
        tile_size = 32,
        bounces = 20,
        light_samples = 1,
        light_bounces = 256,
    },

    camera = camera.perspective {
        fov = 27,
        transform = transform.look_at {
            from = vector {x = -40, y = -30, z = 20},
            to = vector {z = 4.7},
            up = vector {z = 1},
        },
    },

    world = {
        objects = {
            shape.mesh {
                file = "dragon.obj",

                materials = {
                    dragon = material.refractive {
                        ior = 1.5,
                        _ior = 2.37782,
                        dispersion = 0.01371,
                        color = 1,
                    },
                },

                transform = transform.look_at {
                    from = vector(),
                    to = vector(0, 0, -1),
                    up = vector(8, 2, 0),
                },
            },

            shape.plane {
                origin = vector(),
                normal = vector {z = 1},
                material = material.diffuse {color = 0.4},
            },

            shape.plane {
                origin = vector {y = -10},
                normal = vector {y = -1},
                material = material.diffuse {color = 0.4},
            },

            shape.plane {
                origin = vector {x = -11},
                normal = vector {x = 1},
                material = material.diffuse {color = 0.4},
            },

            light.point {
                position = vector {x = 10, y = -25, z = 60},
                direction = vector {x = -10, y = 25, z = -57},
                beam_angle = 6,
                color = light_source.d65 * 5000,
                width = 0.53,
            },
        },
    },
}
