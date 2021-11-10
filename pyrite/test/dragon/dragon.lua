return {
    image = {width = 512, height = 300},

    renderer = renderer.photon_mapping {
        pixel_samples = 100,
        spectrum_samples = 10,
        spectrum_bins = 50,
        tile_size = 32,
        bounces = 100,
        light_samples = 1,
        light_bounces = 256,
        initial_radius = 0.2,
        photons = 500000,
        iterations = 2000,
    },

    camera = camera.perspective {
        fov = 27,
        transform = transform.look_at {
            from = vector {x = -40, y = -30, z = 20},
            to = vector {x = 0, z = 3.5},
            up = vector {z = 1},
        },
    },

    world = {
        objects = {
            shape.mesh {
                file = "dragon.obj",

                materials = {
                    dragon = {
                        surface = {
                            reflection = material.mirror {
                                ior = 1.5,
                                -- ior = 2.37782,
                                -- dispersion = 0.01371,
                                color = 0.8,
                            },
                        },
                    },
                },

                transform = transform.look_at {
                    from = vector(),
                    to = vector(0, 0, -1),
                    up = vector(1, 0, 0),
                },
            },

            shape.plane {
                origin = vector(),
                normal = vector {z = 1},
                material = {
                    surface = {reflection = material.diffuse {color = 0.8}},
                },
            },

            shape.plane {
                origin = vector {y = -10},
                normal = vector {y = 1},
                material = {
                    surface = {
                        reflection = material.diffuse {
                            color = 0.5 * spectrum {
                                format = "curve",
                                points = {
                                    {350, 0},
                                    {400, 0.2},
                                    {550, 0.2},
                                    {600, 0.8},
                                    {750, 0.4},
                                    {850, 0.35},
                                    {1000, 0.45},
                                },
                            },
                        },
                    },
                },
            },

            shape.plane {
                origin = vector {x = -11},
                normal = vector {x = 1},
                material = {
                    surface = {
                        reflection = material.diffuse {
                            color = (0.8 / 0.6) * spectrum {
                                format = "curve",
                                points = {
                                    {350, 0},
                                    {400, 0.1},
                                    {450, 0.1},
                                    {550, 0.3},
                                    {575, 0.1},
                                    {700, 0.1},
                                    {800, 0.45},
                                    {900, 0.6},
                                    {1000, 0.5},
                                },
                            },
                        },
                    },
                },
            },

            shape.plane {
                origin = vector {x = 41},
                normal = vector {x = -1},
                material = {
                    surface = {reflection = material.diffuse {color = 0.8}},
                },
            },

            shape.plane {
                origin = vector {y = 31},
                normal = vector {y = -1},
                material = {
                    surface = {reflection = material.diffuse {color = 0.8}},
                },
            },

            shape.plane {
                origin = vector {z = -40},
                normal = vector {z = -1},
                material = {
                    surface = {reflection = material.diffuse {color = 0.8}},
                },
            },

            --[[light.point {
                position = vector {x = -10, y = -25, z = 60},
                direction = vector {x = 10, y = 25, z = -57},
                beam_angle = 6,
                color = light_source.d65 * 5,
                width = 0.53,
            },]]

            --[[shape.sphere {
                radius = 0.1,
                position = vector {x = 2, y = -25, z = 15},
                material = {surface = {emission = light_source.d65 * 40000}},
            },]]
            shape.sphere {
                radius = 0.1,
                position = vector {x = -10, y = 0, z = 30},
                material = {surface = {emission = light_source.d65 * 40000}},
            },
        },
    },
}
