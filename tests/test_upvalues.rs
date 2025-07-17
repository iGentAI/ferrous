use ferrous::lua::{State, LuaResult};

#[test]
fn test_simple_upvalue() -> LuaResult<()> {
    let mut state = State::new();
    
    let code = r#"
        local x = 42
        
        function get_x()
            return x
        end
        
        return get_x()
    "#;
    
    let results = state.execute(code)?;
    
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].to_number(), Some(42.0));
    
    Ok(())
}

#[test]
fn test_nested_upvalue() -> LuaResult<()> {
    let mut state = State::new();
    
    let code = r#"
        local x = 10
        
        function outer()
            local y = 20
            
            function inner()
                return x + y
            end
            
            return inner()
        end
        
        return outer()
    "#;
    
    let results = state.execute(code)?;
    
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].to_number(), Some(30.0));
    
    Ok(())
}

#[test]
fn test_upvalue_mutation() -> LuaResult<()> {
    let mut state = State::new();
    
    let code = r#"
        local counter = 0
        
        function increment()
            counter = counter + 1
            return counter
        end
        
        local a = increment()
        local b = increment()
        local c = increment()
        
        return a, b, c, counter
    "#;
    
    let results = state.execute(code)?;
    
    assert_eq!(results.len(), 4);
    assert_eq!(results[0].to_number(), Some(1.0));
    assert_eq!(results[1].to_number(), Some(2.0));
    assert_eq!(results[2].to_number(), Some(3.0));
    assert_eq!(results[3].to_number(), Some(3.0));
    
    Ok(())
}

#[test]
fn test_closure_with_upvalue() -> LuaResult<()> {
    let mut state = State::new();
    
    let code = r#"
        local function make_counter(start)
            local count = start
            
            return function()
                count = count + 1
                return count
            end
        end
        
        local c1 = make_counter(0)
        local c2 = make_counter(100)
        
        local a = c1()  -- 1
        local b = c1()  -- 2
        local c = c2()  -- 101
        local d = c1()  -- 3
        local e = c2()  -- 102
        
        return a, b, c, d, e
    "#;
    
    let results = state.execute(code)?;
    
    assert_eq!(results.len(), 5);
    assert_eq!(results[0].to_number(), Some(1.0));
    assert_eq!(results[1].to_number(), Some(2.0));
    assert_eq!(results[2].to_number(), Some(101.0));
    assert_eq!(results[3].to_number(), Some(3.0));
    assert_eq!(results[4].to_number(), Some(102.0));
    
    Ok(())
}