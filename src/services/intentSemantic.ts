export type SemanticIntent = "ASK" | "DO";

export type SemanticIntentMatch = {
  taskType: SemanticIntent;
  reason: string;
};

function normalizeIntentText(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/\s+/g, " ");
}

function containsAny(text: string, patterns: RegExp[]): boolean {
  return patterns.some((pattern) => pattern.test(text));
}

/*
 * Pure semantic classifier.
 *
 * It answers only:
 * "Does this request require AI-OS to interact with real/local/external state?"
 *
 * It does not select or authorize a concrete Skill capability.
 */
export function classifySemanticIntent(
  userRequest: string,
): SemanticIntentMatch | undefined {
  const text = normalizeIntentText(userRequest);

  if (!text) {
    return {
      taskType: "ASK",
      reason: "Empty requests are not executable work.",
    };
  }

  const conceptualAskPatterns: RegExp[] = [
    /^(什么是|什么叫|何为|介绍一下|解释一下|解释|为什么|为何|怎么理解)/u,
    /(有什么区别|有何区别|区别是什么|优缺点|利弊|区别在哪)/u,
    /(你觉得|你认为|你怎么看|怎么看待|有什么看法)/u,
    /(建议我|给我建议|有什么建议|怎么规划|如何规划|怎么分类比较好|应该怎么)/u,
    /(分析一下|帮我分析|比较一下|对比一下)/u,
    /\b(what is|why is|explain|compare|comparison|pros and cons|advise|advice|recommendation)\b/i,
  ];

  const explicitExecutionPatterns: RegExp[] = [
    /^(帮我|替我|给我|请|麻烦).*(查看|看看|检查|读取|扫描|搜索|查找|打开|运行|执行|启动|创建|生成|写入|保存|修改|编辑|移动|复制|重命名|删除|下载|上传|安装|卸载|发送|发邮件|添加|更新|整理|清理|连接|控制|关闭|开启|同步|导出|导入)/u,
    /^(查看|看看|检查|读取|扫描|搜索|查找|打开|运行|执行|启动|创建|生成|写入|保存|修改|编辑|移动|复制|重命名|删除|下载|上传|安装|卸载|发送|添加|更新|整理|清理|连接|控制|关闭|开启|同步|导出|导入)/u,
    /\b(check|inspect|read|scan|search|find|open|run|execute|start|create|write|save|modify|edit|move|copy|rename|delete|download|upload|install|uninstall|send|add|update|organize|clean|connect|control|sync|export|import)\b/i,
  ];

  const realStatePatterns: RegExp[] = [
    /(我的|当前|现在|实时|本机|这台电脑|这台mac|本地|系统里|设备上|账户里|账号里)/u,
    /\b(my|current|right now|local|on this mac|on my mac|in my account|on my device)\b/i,
  ];

  const systemObjectPatterns: RegExp[] = [
    /(nas|硬盘|磁盘|存储|空间|容量|文件|文件夹|downloads|下载目录|邮件|邮箱|日历|浏览器|网页|模型|ollama|omlx|comfyui|应用|程序|电脑|mac|系统|服务器|账户|账号)/iu,
    /\b(nas|disk|drive|storage|file|folder|downloads|email|mail|calendar|browser|webpage|model|ollama|omlx|comfyui|application|computer|server|account)\b/i,
  ];

  const advisoryExecutionConflictPatterns: RegExp[] = [
    /(如果|假如|假设|要是).*(你建议|建议我|怎么|如何)/u,
    /(你建议|有什么建议|怎么规划|如何规划|怎么分类比较好|应该怎么)/u,
    /\b(if i|if we|what should i|how should i|would you recommend)\b/i,
  ];

  if (
    containsAny(text, advisoryExecutionConflictPatterns) ||
    containsAny(text, conceptualAskPatterns)
  ) {
    return {
      taskType: "ASK",
      reason:
        "The request asks for explanation, analysis, comparison, or advice rather than execution.",
    };
  }

  if (
    containsAny(text, explicitExecutionPatterns) &&
    containsAny(text, systemObjectPatterns)
  ) {
    return {
      taskType: "DO",
      reason:
        "The request explicitly asks AI-OS to interact with a real system or object.",
    };
  }

  const inspectionPatterns: RegExp[] = [
    /(查看|看看|检查|读取|扫描|搜索|查找|还有多少|剩多少|可用空间|当前状态|现在状态)/u,
    /\b(check|inspect|look at|see how much|how much.*left|current status|available space)\b/i,
  ];

  if (
    containsAny(text, inspectionPatterns) &&
    containsAny(text, realStatePatterns) &&
    containsAny(text, systemObjectPatterns)
  ) {
    return {
      taskType: "DO",
      reason:
        "The request asks for current information that requires reading real system state.",
    };
  }

  return undefined;
}
