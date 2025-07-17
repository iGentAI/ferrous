-- Minimal string concatenation test
local s1 = "test"
local s2 = "test"
local s3 = "te".."st"

print("s1 == s2:", s1 == s2)
print("s1 == s3:", s1 == s3)

return s1 == s2 and s1 == s3
