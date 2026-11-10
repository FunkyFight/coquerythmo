//! Training data for Needle 3, written by the app's own code.
//!
//! `tools/needle3_finetune/generate_queries.py` writes sentences with their
//! expected calls and session facts; this exporter adds, for each sentence,
//! the tools the app would really send ([`super::retrieval`]) and the system
//! text it would really write ([`super::session::SessionFacts`]). The model
//! is then trained on exactly what it reads in the app.
//!
//! ```text
//! NEEDLE_TRAIN_IN=queries.jsonl NEEDLE_TRAIN_OUT=train.jsonl \
//!   cargo test --lib ai::training -- --ignored --nocapture
//! ```
//!
//! Sentences whose expected tool is not among the tools sent are written to
//! `<out>.missed.jsonl`: they show triggers to add in `tools.rs`.

#[cfg(test)]
mod export {
    use super::super::{retrieval, session::SessionFacts, tools};
    use std::io::{BufRead, Write};

    #[derive(serde::Deserialize)]
    struct Sample {
        query: String,
        answers: Vec<serde_json::Value>,
        #[serde(default)]
        reasoning: String,
        #[serde(default)]
        facts: SessionFacts,
    }

    #[test]
    #[ignore = "writes training data, needs NEEDLE_TRAIN_IN and NEEDLE_TRAIN_OUT"]
    fn export_training_data() {
        let (Ok(input), Ok(output)) = (std::env::var("NEEDLE_TRAIN_IN"), std::env::var("NEEDLE_TRAIN_OUT")) else {
            return;
        };
        let reader = std::io::BufReader::new(std::fs::File::open(&input).expect("input"));
        let mut out = std::io::BufWriter::new(std::fs::File::create(&output).expect("output"));
        let mut missed = std::io::BufWriter::new(
            std::fs::File::create(format!("{output}.missed.jsonl")).expect("missed output"),
        );
        let (mut written, mut skipped, mut alone) = (0usize, 0usize, 0usize);
        for line in reader.lines() {
            let line = line.expect("line");
            if line.trim().is_empty() {
                continue;
            }
            let sample: Sample = serde_json::from_str(&line).expect("sample");
            let sent = retrieval::select_for_query(&sample.query);
            let names: Vec<&str> = sent.iter().map(|tool| tool.name).collect();
            let expected: Vec<&str> = sample
                .answers
                .iter()
                .filter_map(|answer| answer["name"].as_str())
                .collect();
            if !expected.iter().all(|name| names.contains(name)) {
                skipped += 1;
                let row = serde_json::json!({"query": sample.query, "expected": expected, "sent": names});
                writeln!(missed, "{row}").unwrap();
                continue;
            }
            if sent.len() == 1 {
                alone += 1;
            }
            let schemas: Vec<serde_json::Value> = sent.iter().map(|tool| tools::schema(tool)).collect();
            let row = serde_json::json!({
                "system": sample.facts.to_system_prompt(),
                "tools": schemas,
                "query": sample.query,
                "reasoning": sample.reasoning,
                "answers": sample.answers,
            });
            writeln!(out, "{row}").unwrap();
            written += 1;
        }
        println!("training rows: {written} written ({alone} with a single tool), {skipped} missed by retrieval");
    }
}
