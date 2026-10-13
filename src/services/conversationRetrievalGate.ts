/*
 * Cross-conversation retrieval relevance gate.
 *
 * Current conversation messages are NOT controlled by this gate.
 * The gate only determines whether other saved conversations
 * should be searched for additional context.
 */
export function shouldRetrieveConversationHistory(
  request: string,
): boolean {
  const text = request.trim();

  if (!text) return false;

  // Clear requests referring to older conversations.
  const crossConversationPatterns = [
    /(?:上次|上回|以前|此前|过去|前几天|历史聊天|历史对话|其他对话|之前的聊天|之前的对话|之前我们|之前你)/u,
    /(?:previous conversation|earlier conversation|last time|past chats?|chat history|we discussed before)/i,
  ];

  if (crossConversationPatterns.some((pattern) => pattern.test(text))) {
    return true;
  }

  // Greetings and short conversational acknowledgements.
  if (
    /^(?:hi|hello|hey|thanks|thank you|ok|okay|你好|嗨|谢谢|好的|明白了)[\s！!。.？?]*$/iu.test(text)
  ) {
    return false;
  }

  // Questions about the current conversation should rely on
  // current-chat context rather than cross-chat retrieval.
  const currentConversationPatterns = [
    /(?:刚才|刚刚|上一条|上面|前面|这段对话|本次对话|当前对话|这次聊天)/u,
    /(?:just now|earlier in this chat|above|previous message|this conversation)/i,
  ];

  if (
    currentConversationPatterns.some((pattern) => pattern.test(text))
  ) {
    return false;
  }

  // Plain status statements must not pull in earlier topics.
  const looksLikeQuestion =
    /[？?]/u.test(text) ||
    /(?:怎么|如何|为什么|为何|哪些|哪种|哪个好|能否|可否|是否|怎么办|有什么|需要什么|应该|推荐|建议|解释|比较|对比|分析)/u.test(text) ||
    /\b(?:how|why|what|which|should|could|would|compare|explain|recommend)\b/i.test(text);

  if (!looksLikeQuestion) {
    return false;
  }

  // Skip very short or underspecified questions.
  // More substantial topical questions may benefit from history.
  return text.length >= 8;
}
