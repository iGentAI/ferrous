-- Redis Lua Integration Test
-- Tests Redis-specific Lua functionality
-- This test simulates the Redis-specific functions and tables

-- Create mock redis environment
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

-- Create _G.KEYS and _G.ARGV
_G.KEYS = KEYS
_G.ARGV = ARGV
_G.redis = redis

-- Test redis.call GET
local value = redis.call("GET", KEYS[1])
print("GET result:", value)

-- Test redis.call SET
local set_result = redis.call("SET", KEYS[1], ARGV[1])
print("SET result:", set_result)

-- Test redis.call with multiple arguments
local hmset_result = redis.call("HMSET", KEYS[2], "field1", ARGV[1], "field2", ARGV[2])
print("HMSET result:", hmset_result)

-- Test redis.pcall with valid command
local pcall_result = redis.pcall("HGETALL", KEYS[2])
if type(pcall_result) == "table" then
  print("HGETALL result:", table.concat(pcall_result, ", "))
else
  print("HGETALL result:", pcall_result)
end

-- Test redis.pcall with error handling
local pcall_error = redis.pcall("INVALID_COMMAND")
if pcall_error and pcall_error.err then
  print("Error correctly handled:", pcall_error.err)
end

-- Test script access to KEYS and ARGV tables
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

-- This test passes just by running successfully - actual Redis integration
-- will need to be added to the VM
return true