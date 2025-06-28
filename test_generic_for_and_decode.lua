local t = {a=1, b=2, c=3}

-- Test generic for loop with pairs
local pairs_result = {}
for k, v in pairs(t) do
    pairs_result[k] = v
end

-- Test cjson.decode
local json_str = '{"name":"test","values":[1,2,3],"nested":{"key":"value"}}'
local json_data = cjson.decode(json_str)

return {pairs_test = pairs_result, decode_test = json_data}
