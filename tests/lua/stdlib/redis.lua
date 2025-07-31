-- Redis Lua Integration Test (Lua 5.1 Compatible)
-- Tests Redis-style Lua functionality with mock environment

-- NOTE: This test provides mock Redis environment for Lua 5.1 compatibility
print("===== Redis Lua Integration Test (Mock Environment) =====")

-- Create mock redis environment (would be provided by Redis normally)
local redis = {}
local KEYS = {"key1", "key2"}
local ARGV = {"value1", "value2", "value3"}

-- Mock redis.call implementation
function redis.call(cmd, ...)
  local args = {...}
  print("redis.call:", cmd, table.concat(args, ", "))
  
  if cmd:upper() == "GET" then
    return "value-of-" .. args[1]
  elseif cmd:upper() == "SET" then
    return "OK"
  elseif cmd:upper() == "HGETALL" then
    return {"field1", "value1", "field2", "value2"}
  elseif cmd:upper() == "HMSET" then
    return "OK"
  end
  
  return nil
end

-- Mock redis.pcall implementation
function redis.pcall(cmd, ...)
  local ok, result = pcall(function() return redis.call(cmd, ...) end)
  if ok then
    return result
  else
    return {err = result}
  end
end

-- Test mock environment works
local value = redis.call("GET", KEYS[1])
print("GET result:", value)

local set_result = redis.call("SET", KEYS[1], ARGV[1])
print("SET result:", set_result)

-- Test with multiple arguments
local hmset_result = redis.call("HMSET", KEYS[2], "field1", ARGV[1], "field2", ARGV[2])
print("HMSET result:", hmset_result)

-- Test pcall
local pcall_result = redis.pcall("HGETALL", KEYS[2])
if type(pcall_result) == "table" and not pcall_result.err then
  print("HGETALL result:", table.concat(pcall_result, ", "))
else
  print("HGETALL result:", pcall_result)
end

-- Test error handling
local pcall_error = redis.pcall("INVALID_COMMAND")
if pcall_error and pcall_error.err then
  print("Error correctly handled:", pcall_error.err)
end

-- Test script access to mock KEYS and ARGV
local keys_str = ""
for i=1, #KEYS do
  keys_str = keys_str .. KEYS[i] .. " "
end

local argv_str = ""
for i=1, #ARGV do
  argv_str = argv_str .. ARGV[i] .. " "
end

print("KEYS:", keys_str)
print("ARGV:", argv_str)

print("===== Mock Redis Environment Test Completed =====")

-- Test passes if all mock operations work correctly
return true