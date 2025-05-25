#[cfg(test)]
mod cas_tests {

    #[test]
    fn get_cas_names_success() {

        let _ = env_logger::try_init();

        match  ibmad::cas::get_cas_names(){
            Ok(cas) =>{
                assert!( cas.len() > 0, "No CAs found.");
                for _ca in cas.iter(){
                    // 
                }
            }
            Err(e) => {
                assert!(false, "{}", format!("Error finding CAs: {:?}", e));
            }
        }
    }

    #[test]
    fn get_cas_success() {
        
        let _ = env_logger::try_init();

        match  ibmad::cas::get_cas(){
            Ok(cas) =>{
                assert!(cas.len() > 0, "No CAs found.");
            }
            Err(e) => {
                assert!(false, "{}", format!("Error finding CAs: {:?}", e));
            }
        }
    }
}
