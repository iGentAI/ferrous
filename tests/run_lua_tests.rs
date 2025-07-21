    #[test]
    fn test_upvalue_simple() -> Result<(), Box<dyn std::error::Error>> {
        let test_file = "tests/lua/functions/upvalue_simple.lua";
        run_single_test(&test_file)?;
        Ok(())
    }