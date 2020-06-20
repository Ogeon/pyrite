_pyrite = {
    binary_operator = function(type, a, b)
        local properties = {type = type, a = a, b = b}
        setmetatable(properties, _pyrite.expression_mt)
        return properties
    end,
}

function dump(o, t)
    local tabs = t or 1

    if type(o) == "table" then
        local s = "{\n"
        for k, v in pairs(o) do
            for _ = 1, tabs do s = s .. "  " end
            if type(k) ~= "number" then k = "\"" .. k .. "\"" end
            s = s .. "[" .. k .. "] = " .. dump(v, tabs + 1) .. ",\n"
        end
        for _ = 1, tabs - 1 do s = s .. "  " end
        return s .. "}"
    else
        return tostring(o)
    end
end

-- Basics

_pyrite.basics_mt = {}
_pyrite.basics_mt.__index = _pyrite.basics_mt

-- Shallow clones a table.
function _pyrite.basics_mt:clone()
    local self_type = type(self)

    local cloned
    if self_type == "table" then
        cloned = {}
        for key, value in pairs(self) do cloned[key] = value end
        setmetatable(cloned, getmetatable(self))
    else
        cloned = self
    end

    return cloned
end

-- Like clone, but with changes.
function _pyrite.basics_mt:with(new_properties)
    local cloned = self:clone();
    local new_properties_table

    if type(new_properties) == "function" then
        new_properties_table = new_properties(cloned)
    else
        new_properties_table = new_properties
    end

    for key, value in pairs(new_properties_table) do cloned[key] = value end

    return cloned
end

-- Expression

_pyrite.expression_mt = setmetatable({}, {__index = _pyrite.basics_mt})
_pyrite.expression_mt.__index = _pyrite.expression_mt

function _pyrite.expression_mt:__add(other)
    return _pyrite.binary_operator("add", self, other)
end

function _pyrite.expression_mt:__sub(other)
    return _pyrite.binary_operator("sub", self, other)
end

function _pyrite.expression_mt:__mul(other)
    return _pyrite.binary_operator("mul", self, other)
end

function _pyrite.expression_mt:__div(other)
    return _pyrite.binary_operator("div", self, other)
end

function _pyrite.expression_mt:fresnel_mix(other, ior)
    local properties

    if type(self) == "table" and self.type == nil then
        properties = self
        properties.type = "fresnel_mix"
    else
        properties = {
            type = "fresnel_mix",
            reflect = self,
            refract = other,
            ior = ior,
        }
    end
    setmetatable(properties, _pyrite.expression_mt)

    return properties
end
fresnel_mix = _pyrite.expression_mt.fresnel_mix

function _pyrite.expression_mt:mix(other, factor)
    local properties

    if type(self) == "table" and self.type == nil then
        properties = self
        properties.type = "mix"
    else
        properties = {type = "mix", a = self, b = other, factor = factor}
    end
    setmetatable(properties, _pyrite.expression_mt)

    return properties
end
mix = _pyrite.expression_mt.mix

function fresnel(ior, env_ior)
    local properties = {type = "fresnel", ior = ior, env_ior = env_ior}
    setmetatable(properties, _pyrite.expression_mt)

    return properties
end

-- Vector of up to four elements.
function vector(x, y, z, w)
    local properties

    if type(x) == "table" and x.type == nil then
        properties = {
            type = "vector",
            x = x.x or 0.0,
            y = x.y or 0.0,
            z = x.z or 0.0,
            w = x.w or 0.0,
        }
    else
        properties = {
            type = "vector",
            x = x or 0.0,
            y = y or 0.0,
            z = z or 0.0,
            w = w or 0.0,
        }
    end
    setmetatable(properties, _pyrite.expression_mt)

    return properties
end

function spectrum(points)
    local properties = {type = "spectrum", points = points}
    setmetatable(properties, _pyrite.expression_mt)

    return properties
end

function rgb(red, green, blue)
    local properties = {
        type = "rgb",
        red = red or 0.0,
        green = green or 0.0,
        blue = blue or 0.0,
    }
    setmetatable(properties, _pyrite.expression_mt)

    return properties
end

function texture(path)
    local properties = {type = "texture", path = path}
    setmetatable(properties, _pyrite.expression_mt)

    return properties
end

shape = {
    sphere = function(properties)
        properties.type = "sphere"
        setmetatable(properties, _pyrite.basics_mt)
        return properties
    end,
    plane = function(properties)
        properties.type = "plane"
        setmetatable(properties, _pyrite.basics_mt)
        return properties
    end,
    mesh = function(properties)
        properties.type = "mesh"
        setmetatable(properties, _pyrite.basics_mt)
        return properties
    end,
    ray_marched = function(properties)
        properties.type = "ray_marched"
        setmetatable(properties, _pyrite.basics_mt)
        return properties
    end,
}

ray_marched = {
    quaternion_julia = function(properties)
        properties.type = "quaternion_julia"
        setmetatable(properties, _pyrite.basics_mt)
        return properties
    end,
    mandelbulb = function(properties)
        properties.type = "mandelbulb"
        setmetatable(properties, _pyrite.basics_mt)
        return properties
    end,
}

quaternion_julia = {}
quaternion_julia.cubic = {type = "quaternion_julia", name = "cubic"}
setmetatable(quaternion_julia.cubic, _pyrite.basics_mt)

bounds = {
    box = function(properties)
        properties.type = "box"
        setmetatable(properties, _pyrite.basics_mt)
        return properties
    end,
}

material = {
    diffuse = function(properties)
        properties.type = "diffuse"
        setmetatable(properties, _pyrite.expression_mt)
        return properties
    end,
    emission = function(properties)
        properties.type = "emission"
        setmetatable(properties, _pyrite.expression_mt)
        return properties
    end,
    mirror = function(properties)
        properties.type = "mirror"
        setmetatable(properties, _pyrite.expression_mt)
        return properties
    end,
    refractive = function(properties)
        properties.type = "refractive"
        setmetatable(properties, _pyrite.expression_mt)
        return properties
    end,
}

light_source = {}
light_source.d65 = {type = "light_source", name = "d65"}
setmetatable(light_source.d65, _pyrite.expression_mt)

transform = {
    look_at = function(properties)
        properties.type = "look_at"
        setmetatable(properties, _pyrite.basics_mt)
        return properties
    end,
}

camera = {
    perspective = function(properties)
        properties.type = "perspective"
        setmetatable(properties, _pyrite.basics_mt)
        return properties
    end,
}

renderer = {
    simple = function(properties)
        properties.type = "simple"
        setmetatable(properties, _pyrite.basics_mt)
        return properties
    end,
    bidirectional = function(properties)
        properties.type = "bidirectional"
        setmetatable(properties, _pyrite.basics_mt)
        return properties
    end,
    photon_mapping = function(properties)
        properties.type = "photon_mapping"
        setmetatable(properties, _pyrite.basics_mt)
        return properties
    end,
}

light = {
    point = function(properties)
        properties.type = "point_light"
        setmetatable(properties, _pyrite.basics_mt)
        return properties
    end,
}
