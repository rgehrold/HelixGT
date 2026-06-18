export interface FormatInfo {
  id: string;
  label: string;
  category: "sequence" | "alignment" | "annotation" | "variants";
}

export interface ConvertProgress {
  current: number;
  total: number;
  fileName: string;
}

export interface ConvertResultItem {
  inputPath: string;
  outputPath: string;
  records: number;
}

export interface ConvertSummary {
  files: ConvertResultItem[];
  totalRecords: number;
}

export interface DirEntry {
  name: string;
  path: string;
  isDir: boolean;
}