fn main() {
    let p1 = String::from(r"E:\temp\fftest\del_a.txt");
    let p2 = String::from(r"E:\temp\fftest\del_b.txt");
    println!("attempting trash for {p1} {p2}");
    match fastfiler_domain::file_ops::delete_to_trash(vec![p1, p2]) {
        Ok(()) => println!("OK"),
        Err(e) => println!("ERR: {}", e),
    }
}
