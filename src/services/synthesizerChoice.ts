/**
 * Which of several user-named models should synthesize when the user did not
 * say. No model is named here: cloud models rank above local ones (local
 * models on this class of Mac are small, memory-bound 7-9B models), and among
 * equals the one named last wins, matching how users list a coordinator.
 */
export function preferredSynthesizerIndex(
  candidates: Array<{ providerId?: string }>,
): number {
  let chosen = -1;

  candidates.forEach((candidate, index) => {
    const rank = candidate.providerId && candidate.providerId !== "omlx" ? 1 : 0;
    const chosenRank = chosen < 0
      ? -1
      : candidates[chosen].providerId && candidates[chosen].providerId !== "omlx" ? 1 : 0;
    if (rank >= chosenRank) chosen = index;
  });

  return chosen;
}
