#[derive(Debug, Clone)]
pub struct EnsemblReference {
    pub id: String,
    pub label: String,
    pub species: String,
    pub assembly: String,
    pub release: String,
    pub url: String,
    pub compressed_size_mb: u32,
}

/// Reference genomes from NCBI RefSeq (more reliable than Ensembl FTP for direct downloads).
pub const REFERENCE_SOURCE: &str = "NCBI RefSeq";

pub fn list_references() -> Vec<EnsemblReference> {
    vec![
        EnsemblReference {
            id: "homo_sapiens".into(),
            label: "Human (GRCh38.p14)".into(),
            species: "Homo sapiens".into(),
            assembly: "GRCh38.p14".into(),
            release: REFERENCE_SOURCE.into(),
            url: "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCF/000/001/405/GCF_000001405.40_GRCh38.p14/GCF_000001405.40_GRCh38.p14_genomic.fna.gz".into(),
            compressed_size_mb: 950,
        },
        EnsemblReference {
            id: "mus_musculus".into(),
            label: "Mouse (GRCm39)".into(),
            species: "Mus musculus".into(),
            assembly: "GRCm39".into(),
            release: REFERENCE_SOURCE.into(),
            url: "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCF/000/001/635/GCF_000001635.27_GRCm39/GCF_000001635.27_GRCm39_genomic.fna.gz".into(),
            compressed_size_mb: 820,
        },
        EnsemblReference {
            id: "danio_rerio".into(),
            label: "Zebrafish (GRCz11)".into(),
            species: "Danio rerio".into(),
            assembly: "GRCz11".into(),
            release: REFERENCE_SOURCE.into(),
            url: "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCF/000/002/035/GCF_000002035.6_GRCz11/GCF_000002035.6_GRCz11_genomic.fna.gz".into(),
            compressed_size_mb: 540,
        },
        EnsemblReference {
            id: "arabidopsis_thaliana".into(),
            label: "Arabidopsis (TAIR10.1)".into(),
            species: "Arabidopsis thaliana".into(),
            assembly: "TAIR10.1".into(),
            release: REFERENCE_SOURCE.into(),
            url: "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCF/000/001/735/GCF_000001735.4_TAIR10.1/GCF_000001735.4_TAIR10.1_genomic.fna.gz".into(),
            compressed_size_mb: 35,
        },
        EnsemblReference {
            id: "escherichia_coli_k12".into(),
            label: "E. coli K-12 MG1655".into(),
            species: "Escherichia coli".into(),
            assembly: "ASM584v2".into(),
            release: REFERENCE_SOURCE.into(),
            url: "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCF/000/005/845/GCF_000005845.2_ASM584v2/GCF_000005845.2_ASM584v2_genomic.fna.gz".into(),
            compressed_size_mb: 2,
        },
        EnsemblReference {
            id: "saccharomyces_cerevisiae".into(),
            label: "Yeast (R64)".into(),
            species: "Saccharomyces cerevisiae".into(),
            assembly: "R64".into(),
            release: REFERENCE_SOURCE.into(),
            url: "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCF/000/146/045/GCF_000146045.2_R64/GCF_000146045.2_R64_genomic.fna.gz".into(),
            compressed_size_mb: 4,
        },
    ]
}

pub fn find_reference(id: &str) -> Option<EnsemblReference> {
    list_references()
        .into_iter()
        .find(|reference| reference.id == id)
}