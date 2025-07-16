-- Super minimal circular reference test
print("Testing minimal circular reference")

-- Self-reference
local t = {}
t.self = t

-- Verify it works
print("Self-reference works:", t.self == t)

-- Just return true - we've verified circular refs work
return true