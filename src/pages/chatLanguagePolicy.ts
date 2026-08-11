export type ResponseLanguage = "Chinese" | "English";

const preferencePatterns: Record<ResponseLanguage, RegExp[]> = {
  Chinese: [
    /(?:喜欢|偏好|希望|想要|习惯|默认|请|要).{0,12}(?:中文|汉语).{0,12}(?:回答|回复|作答|响应)?/i,
    /\b(?:i\s+)?(?:prefer|want|like|would like)\b.{0,24}\b(?:answers?|responses?|replies?)?\s*(?:in\s+)?chinese\b/i,
    /\b(?:default to|always|please)\s+(?:answer|respond|reply)\s+in\s+chinese\b/i,
  ],
  English: [
    /(?:喜欢|偏好|希望|想要|习惯|默认|请|要).{0,12}(?:英文|英语).{0,12}(?:回答|回复|作答|响应)?/i,
    /\b(?:i\s+)?(?:prefer|want|like|would like)\b.{0,24}\b(?:answers?|responses?|replies?)?\s*(?:in\s+)?english\b/i,
    /\b(?:default to|always|please)\s+(?:answer|respond|reply)\s+in\s+english\b/i,
  ],
};

export function detectMemoryLanguage(
  memories: string[],
): ResponseLanguage | undefined {
  const matches = (language: ResponseLanguage) =>
    memories.some((memory) =>
      preferencePatterns[language].some((pattern) => pattern.test(memory)),
    );
  const chinese = matches("Chinese");
  const english = matches("English");
  return chinese === english ? undefined : chinese ? "Chinese" : "English";
}

export function detectCurrentLanguage(
  content: string,
): ResponseLanguage | undefined {
  const english =
    /(?:这次|本次|当前|这条)?\s*(?:请)?\s*(?:用|以)\s*英文\s*(?:回答|回复|作答)/i.test(content) ||
    /\b(?:answer|respond|reply)(?:\s+to\s+(?:this|the current)\s+(?:message|request))?\s+in\s+english\b/i.test(content);
  const chinese =
    /(?:这次|本次|当前|这条)?\s*(?:请)?\s*(?:用|以)\s*(?:中文|汉语)\s*(?:回答|回复|作答)/i.test(content) ||
    /\b(?:answer|respond|reply)(?:\s+to\s+(?:this|the current)\s+(?:message|request))?\s+in\s+chinese\b/i.test(content);
  return english === chinese ? undefined : english ? "English" : "Chinese";
}

export function applyOutboundLanguage(
  content: string,
  language: ResponseLanguage | undefined,
): string {
  return language
    ? `${content}\n\n[Current response language: ${language}. Answer this request in ${language}.]`
    : content;
}
