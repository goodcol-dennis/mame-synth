fn main() {
    let ymfm_src = "ymfm/src";

    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .include(ymfm_src)
        .file("wrapper/ymfm_wrapper.cpp")
        .file(format!("{}/ymfm_opn.cpp", ymfm_src))
        .file(format!("{}/ymfm_adpcm.cpp", ymfm_src))
        .file(format!("{}/ymfm_ssg.cpp", ymfm_src))
        .compile("ymfm_wrapper");
}
