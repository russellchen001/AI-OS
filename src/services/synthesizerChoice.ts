/**
 * Parameter count (billions) read from a model's id or label using the usual
 * "7B" / "70b" / "30B-A3B" naming; undefined when the name doesn't say.
 * "4bit" is a quantization, not a size.
 */
export function parameterBillions(model: { modelId?: string; label?: string }): number | undefined {
  for (const text of [model.modelId, model.label]) {
    const match = text?.match(/(?:^|[^a-z0-9.])(\d+(?:\.\d+)?)\s*b(?![a-z])/i);
    if (match) return Number(match[1]);
  }
  return undefined;
}

/** Local models this large are treated like cloud models for synthesis. */
export const STRONG_LOCAL_BILLIONS = 30;

function rank(candidate: { providerId?: string; modelId?: string; label?: string }): number {
  if (!candidate.providerId) return 0;
  if (candidate.providerId !== "omlx") return 1;
  return (parameterBillions(candidate) ?? 0) >= STRONG_LOCAL_BILLIONS ? 1 : 0;
}

/**
 * Which of several user-named models should synthesize when the user did not
 * say. No model is named here: cloud models and large local models (30B+,
 * from the model's own size naming) rank above smaller local models, and among
 * equals the one named last wins, matching how users list a coordinator. A
 * machine with a strong local model therefore keeps synthesis local when the
 * user names it last.
 */
export function preferredSynthesizerIndex(
  candidates: Array<{ providerId?: string; modelId?: string; label?: string }>,
): number {
  let chosen = -1;

  candidates.forEach((candidate, index) => {
    if (chosen < 0 || rank(candidate) >= rank(candidates[chosen])) chosen = index;
  });

  return chosen;
}
