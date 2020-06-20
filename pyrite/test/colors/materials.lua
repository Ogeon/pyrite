local glass_template = material.refractive {ior = 1.5, color = spectrum {}}
return {
    light = material.emission {color = light_source.d65 * 5},

    floor = material.diffuse {color = 1},

    glass_400nm = glass_template:with{
        color = glass_template.color:with{
            points = {{370, 0}, {380, 1}, {420, 1}, {430, 0}},
        },
    },

    glass_430nm = glass_template:with{
        color = glass_template.color:with{
            points = {{400, 0}, {410, 1}, {450, 1}, {460, 0}},
        },
    },

    glass_461nm = glass_template:with{
        color = glass_template.color:with{
            points = {{431, 0}, {441, 1}, {481, 1}, {491, 0}},
        },
    },

    glass_492nm = glass_template:with{
        color = glass_template.color:with{
            points = {{462, 0}, {472, 1}, {512, 1}, {522, 0}},
        },
    },

    glass_523nm = glass_template:with{
        color = glass_template.color:with{
            points = {{493, 0}, {503, 1}, {543, 1}, {553, 0}},
        },
    },

    glass_554nm = glass_template:with{
        color = glass_template.color:with{
            points = {{524, 0}, {534, 1}, {574, 1}, {584, 0}},
        },
    },

    glass_585nm = glass_template:with{
        color = glass_template.color:with{
            points = {{555, 0}, {565, 1}, {605, 1}, {615, 0}},
        },
    },

    glass_616nm = glass_template:with{
        color = glass_template.color:with{
            points = {{586, 0}, {596, 1}, {636, 1}, {646, 0}},
        },
    },

    glass_647nm = glass_template:with{
        color = glass_template.color:with{
            points = {{615, 0}, {627, 1}, {667, 1}, {675, 0}},
        },
    },

    glass_678nm = glass_template:with{
        color = glass_template.color:with{
            points = {{648, 0}, {658, 1}, {698, 1}, {708, 0}},
        },
    },

    glass_709nm = glass_template:with{
        color = glass_template.color:with{
            points = {{689, 0}, {699, 1}, {719, 1}, {729, 0}},
        },
    },

    glass_740nm = glass_template:with{
        color = glass_template.color:with{
            points = {{710, 0}, {720, 1}, {760, 1}, {770, 0}},
        },
    },
}
