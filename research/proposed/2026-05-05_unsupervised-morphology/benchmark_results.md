# Segmenter benchmark results

Generated 2026-05-05 21:46 UTC

Pass criteria from the synthesis:
- training time < 5 minutes on largest fixture
- Viterbi inference < 10 seconds for the full NT
- post-segmentation morpheme bigram hapax ratio < 0.72

| Corpus | Segmenter | Train (s) | Train RSS (MB) | Inference (s) | Morph types | Morph TTR | Morph bigram hapax | Word bigram hapax | Notes |
|---|---|---:|---:|---:|---:|---:|---:|---:|---|
| en_ulb | morfessor-2.0 | 1.4 | 25 | 0.15 | 4349 | 0.0276 | 0.6326 | 0.6463 | |
| en_ulb (English, analytic) | morfessor-em-prune | — | — | — | — | — | — | — | not implemented in this run — clone github.com/Waino/morfessor-emprune, install with `pip install -e .`, then port this file from bench_morfessor.py swapping the `train_batch` call for the EM+Prune trainer. |
| en_ulb (English, analytic) | morphagram-cascaded-prstsu | — | — | — | — | — | — | — | not implemented in this run — MorphAGram is a multi-step shell pipeline. Clone github.com/rnd2110/MorphAGram into experiments/segmenter_benchmark/vendor/MorphAGram, follow its README to train PrStSu+SM on the dump-words TSV, then extend this script to invoke it via subprocess and parse the resulting `*.seg` file into a {word: [morphs]} dict. Re-use the bigram-hapax computation from compute_bigram_hapax. |
| bem_reg | morfessor-2.0 | 15.0 | 34 | 0.86 | 5956 | 0.0412 | 0.7393 | 0.8384 | |
| bem_reg (Bemba, Bantu prefixing) | morfessor-em-prune | — | — | — | — | — | — | — | not implemented in this run — clone github.com/Waino/morfessor-emprune, install with `pip install -e .`, then port this file from bench_morfessor.py swapping the `train_batch` call for the EM+Prune trainer. |
| bem_reg (Bemba, Bantu prefixing) | morphagram-cascaded-prstsu | — | — | — | — | — | — | — | not implemented in this run — MorphAGram is a multi-step shell pipeline. Clone github.com/rnd2110/MorphAGram into experiments/segmenter_benchmark/vendor/MorphAGram, follow its README to train PrStSu+SM on the dump-words TSV, then extend this script to invoke it via subprocess and parse the resulting `*.seg` file into a {word: [morphs]} dict. Re-use the bigram-hapax computation from compute_bigram_hapax. |
| bap-x-rai_reg | morfessor-2.0 | 11.5 | 32 | 0.95 | 6807 | 0.0494 | 0.7748 | 0.8592 | |
| bap-x-rai_reg (Rai, Tibeto-Burman suffixing) | morfessor-em-prune | — | — | — | — | — | — | — | not implemented in this run — clone github.com/Waino/morfessor-emprune, install with `pip install -e .`, then port this file from bench_morfessor.py swapping the `train_batch` call for the EM+Prune trainer. |
| bap-x-rai_reg (Rai, Tibeto-Burman suffixing) | morphagram-cascaded-prstsu | — | — | — | — | — | — | — | not implemented in this run — MorphAGram is a multi-step shell pipeline. Clone github.com/rnd2110/MorphAGram into experiments/segmenter_benchmark/vendor/MorphAGram, follow its README to train PrStSu+SM on the dump-words TSV, then extend this script to invoke it via subprocess and parse the resulting `*.seg` file into a {word: [morphs]} dict. Re-use the bigram-hapax computation from compute_bigram_hapax. |

## Per-cell raw output

### en_ulb — morfessor-2.0

```json
{
  "segmenter": "morfessor-2.0",
  "corpus": "en_ulb",
  "n_input_forms": 5936,
  "n_morpheme_types": 4349,
  "train_seconds": 1.382970292121172,
  "train_peak_rss_mb": 25.125,
  "inference_seconds": 0.14727733377367258,
  "raw_word_stats": {
    "n_tokens": 147297,
    "n_unigram_types": 5372,
    "ttr": 0.03647053232584506,
    "unigram_hapax_ratio": 0.33302308265078184,
    "n_bigram_types": 44678,
    "bigram_hapax_ratio": 0.6463136219168271
  },
  "segmented_morpheme_stats": {
    "n_tokens": 149758,
    "n_unigram_types": 4139,
    "ttr": 0.02763792251499085,
    "unigram_hapax_ratio": 0.19569944431021985,
    "n_bigram_types": 44918,
    "bigram_hapax_ratio": 0.6326417026581771
  }
}
```

### en_ulb (English, analytic) — morfessor-em-prune

```json
{
  "segmenter": "morfessor-em-prune",
  "error": "not implemented in this run — clone github.com/Waino/morfessor-emprune, install with `pip install -e .`, then port this file from bench_morfessor.py swapping the `train_batch` call for the EM+Prune trainer.",
  "corpus": "en_ulb (English, analytic)"
}
```

### en_ulb (English, analytic) — morphagram-cascaded-prstsu

```json
{
  "segmenter": "morphagram-cascaded-prstsu",
  "error": "not implemented in this run — MorphAGram is a multi-step shell pipeline. Clone github.com/rnd2110/MorphAGram into experiments/segmenter_benchmark/vendor/MorphAGram, follow its README to train PrStSu+SM on the dump-words TSV, then extend this script to invoke it via subprocess and parse the resulting `*.seg` file into a {word: [morphs]} dict. Re-use the bigram-hapax computation from compute_bigram_hapax.",
  "corpus": "en_ulb (English, analytic)"
}
```

### bem_reg — morfessor-2.0

```json
{
  "segmenter": "morfessor-2.0",
  "corpus": "bem_reg",
  "n_input_forms": 21735,
  "n_morpheme_types": 5956,
  "train_seconds": 15.012012457940727,
  "train_peak_rss_mb": 34.203125,
  "inference_seconds": 0.8558022920042276,
  "raw_word_stats": {
    "n_tokens": 116538,
    "n_unigram_types": 18771,
    "ttr": 0.16107192503732687,
    "unigram_hapax_ratio": 0.6433328005966651,
    "n_bigram_types": 69855,
    "bigram_hapax_ratio": 0.8384081311287668
  },
  "segmented_morpheme_stats": {
    "n_tokens": 140465,
    "n_unigram_types": 5782,
    "ttr": 0.04116327910867476,
    "unigram_hapax_ratio": 0.12694569353164994,
    "n_bigram_types": 71068,
    "bigram_hapax_ratio": 0.7393200878032307
  }
}
```

### bem_reg (Bemba, Bantu prefixing) — morfessor-em-prune

```json
{
  "segmenter": "morfessor-em-prune",
  "error": "not implemented in this run — clone github.com/Waino/morfessor-emprune, install with `pip install -e .`, then port this file from bench_morfessor.py swapping the `train_batch` call for the EM+Prune trainer.",
  "corpus": "bem_reg (Bemba, Bantu prefixing)"
}
```

### bem_reg (Bemba, Bantu prefixing) — morphagram-cascaded-prstsu

```json
{
  "segmenter": "morphagram-cascaded-prstsu",
  "error": "not implemented in this run — MorphAGram is a multi-step shell pipeline. Clone github.com/rnd2110/MorphAGram into experiments/segmenter_benchmark/vendor/MorphAGram, follow its README to train PrStSu+SM on the dump-words TSV, then extend this script to invoke it via subprocess and parse the resulting `*.seg` file into a {word: [morphs]} dict. Re-use the bigram-hapax computation from compute_bigram_hapax.",
  "corpus": "bem_reg (Bemba, Bantu prefixing)"
}
```

### bap-x-rai_reg — morfessor-2.0

```json
{
  "segmenter": "morfessor-2.0",
  "corpus": "bap-x-rai_reg",
  "n_input_forms": 20075,
  "n_morpheme_types": 6807,
  "train_seconds": 11.516338916961104,
  "train_peak_rss_mb": 32.046875,
  "inference_seconds": 0.9533310001716018,
  "raw_word_stats": {
    "n_tokens": 113775,
    "n_unigram_types": 20075,
    "ttr": 0.17644473742034716,
    "unigram_hapax_ratio": 0.6365130759651307,
    "n_bigram_types": 77327,
    "bigram_hapax_ratio": 0.859234161418392
  },
  "segmented_morpheme_stats": {
    "n_tokens": 137707,
    "n_unigram_types": 6807,
    "ttr": 0.04943103836406283,
    "unigram_hapax_ratio": 0.17467313060085207,
    "n_bigram_types": 82728,
    "bigram_hapax_ratio": 0.7747920897398705
  }
}
```

### bap-x-rai_reg (Rai, Tibeto-Burman suffixing) — morfessor-em-prune

```json
{
  "segmenter": "morfessor-em-prune",
  "error": "not implemented in this run — clone github.com/Waino/morfessor-emprune, install with `pip install -e .`, then port this file from bench_morfessor.py swapping the `train_batch` call for the EM+Prune trainer.",
  "corpus": "bap-x-rai_reg (Rai, Tibeto-Burman suffixing)"
}
```

### bap-x-rai_reg (Rai, Tibeto-Burman suffixing) — morphagram-cascaded-prstsu

```json
{
  "segmenter": "morphagram-cascaded-prstsu",
  "error": "not implemented in this run — MorphAGram is a multi-step shell pipeline. Clone github.com/rnd2110/MorphAGram into experiments/segmenter_benchmark/vendor/MorphAGram, follow its README to train PrStSu+SM on the dump-words TSV, then extend this script to invoke it via subprocess and parse the resulting `*.seg` file into a {word: [morphs]} dict. Re-use the bigram-hapax computation from compute_bigram_hapax.",
  "corpus": "bap-x-rai_reg (Rai, Tibeto-Burman suffixing)"
}
```
