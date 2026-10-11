# Evidence-led subject discovery

The subject index is independent of the coding-project registry. Projects, people, interests, devices and ideas are all knowledge subjects. The browser classifies saved evidence; it does not scrape or count every private chat message. The shared assistant workflow captures useful knowledge from conversations and work.

## Research and choice

| Approach | Useful property | Limitation here |
|---|---|---|
| [GraphRAG](https://microsoft.github.io/graphrag/index/default_dataflow/) | Separates text, entities, relationships and communities. | A full extraction/summarization pipeline adds background model calls and operational cost. |
| [BERTopic hierarchy](https://maartengr.github.io/BERTopic/getting_started/hierarchicaltopics/hierarchicaltopics.html) | Topic-term representations support coherent clusters and hierarchy. | Unconstrained reclustering can rename/rearrange a small personal collection. |
| [Sentence Transformers clustering](https://www.sbert.net/examples/sentence_transformer/applications/clustering/README.html) | Semantic similarity connects different wording. | Similarity alone cannot establish a dependency, correct identity, or factual confidence. |
| [spaCy entity rules](https://spacy.io/api/entityruler) | Known aliases/declarations provide precise anchors. | Hand-maintained project keyword lists do not discover new interests and misclassify generic terms. |
| [FastEmbed](https://github.com/qdrant/fastembed) | CPU ONNX embeddings without a large PyTorch/GPU stack. | English embedding quality and context length remain constraints. |

The implementation combines explicit data-led anchors with local semantic similarity, discriminative lexical features and coherent communities. There are no hardcoded subject names or project-to-keyword shortcuts. Broad display-category icons remain cosmetic; they do not decide which project owns a note.

## Assignment and growth

1. Parse actual current wiki declarations, short curated overview titles, explicit Worlds/Topics and legacy Project annotations. These are data, not an application project registry.
2. Resolve declared aliases. Curated evidence references and prominent named evidence bootstrap old notes. Ambiguous namespace prefixes and preservation/example boilerplate are not personal/name matches.
3. Learn subject profiles from grounded anchors. Combine local semantic similarity with TF-IDF features. Close competing matches remain suggested/unresolved instead of being silently forced into one subject.
4. Repeated coherent evidence can discover a new subject with no descriptor or registered repository. Three distinct meaningful notes are needed for repeated named subjects. Specific new names are detected before prototype assignment so a new project is not swallowed by an existing similar subject; generic communities require stronger support. Identical templates are not independent evidence.
5. Smaller coherent groups remain suggested topics under a related subject. Average-link community merging has a weakest-pair guard to avoid one bridging note collapsing unrelated topics.
6. Topics can become major subjects after at least five independent dated observations across two days and substantial relative focus. Once promoted, their identity remains when the parent grows and activity fades.
7. Explicit user corrections take priority, become profile anchors, and are reversible. They change a private derived override, not the original note or its factual confidence.

Automatic groups are associations. Explicit note references remain separate directed evidence links. Grouping confidence is not factual verification.

## Activity and identity

Recent focus uses meaningful recorded evidence, a 21-day half-life, deduplication and logarithmic daily returns. More independent captures matter; one day of repetitive output does not overwhelm the graph. It does not infer activity from file modification times. Old knowledge stays accessible.

Canonical labels/aliases and member overlap preserve IDs across rebuilds. Promoted subjects are retained through changing relative activity. Metadata from raw imports/code examples cannot declare curated entities.

## Runtime and privacy

BAAI/bge-small-en-v1.5 is a 384-dimensional CPU model listed at approximately 67MB in [FastEmbed's model inventory](https://qdrant.github.io/fastembed/examples/Supported_Models/). Dependencies are isolated under ~/.local/share/nacre/brain-model; weights are downloaded once. Normal inference uses local_files_only and HF_HUB_OFFLINE. No note text is uploaded. Documents enter the worker over stdin, never command arguments or logs.

Private derived state lives under the vault's hidden .brain-state/discovery directory: revision-keyed vectors, stable discovered identities and grouping overrides. Files use 0600. Public Git contains code and synthetic tests only. Original Markdown remains append-only unless the user explicitly edits it through the existing maintenance workflow.

At the initial subject-discovery rollout (`543ee68`), inference on that collection
took about 38 seconds; a warm model build measured about 0.6 seconds. These are
historical measurements, not current-host latency guarantees. Subsequent
in-process graph requests are cached. Embeddings run in batches and only changed revisions are re-encoded. If unavailable, explicit/lexical grouping remains functional and reports the fallback.

## Evaluation and limits

Regression fixtures cover unseen subjects with no registry, overlapping annotations, ambiguous namespaces, duplicate-template rejection, automatic growth, stable identities, promotion, durable manual overrides and path/privacy boundaries. A held-out local semantic probe assigned an electronics supply-drop observation to Electronics and left an unrelated portrait study unassigned. Live assignment audits retain a Needs grouping lane for ambiguous cases.

This does not claim perfect automatic categorization. Labels are drawn from evidence rather than generated claims; some can still be awkward. English and mixed-language prose, unusual aliases and sparse subjects can need correction. Notes → Change grouping provides the feedback path. Use automatic grouping removes that override.

The brain launches as a normal tiled app on workspace 6. Initial fullscreen/maximize requests are suppressed for its specific app; startup explicitly clears restored fullscreen state. Other browsers and the user's manual fullscreen shortcuts are unaffected.
