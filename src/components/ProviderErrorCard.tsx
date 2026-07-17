type ProviderErrorCardProps = {
  error: string;
};

type ErrorSummary = {
  title: string;
  description: string;
  statusCode?: string;
};

function summarizeError(
  error: string,
): ErrorSummary {
  const normalized =
    error.toLowerCase();

  const statusCode =
    error.match(
      /\bHTTP\s+(\d{3})\b/i,
    )?.[1];

  if (
    normalized.includes(
      "insufficient balance",
    ) ||
    normalized.includes(
      "insufficient_balance",
    ) ||
    normalized.includes(
      "insufficient quota",
    ) ||
    normalized.includes(
      "insufficient_quota",
    ) ||
    normalized.includes(
      "credit balance is too low",
    )
  ) {
    return {
      title: "No quota",
      description:
        "This provider has insufficient credit or API quota.",
      statusCode,
    };
  }

  if (
    normalized.includes("503") ||
    normalized.includes(
      "service unavailable",
    ) ||
    normalized.includes(
      "high demand",
    )
  ) {
    return {
      title: "Provider busy",
      description:
        "The provider is temporarily unavailable. Try again later.",
      statusCode,
    };
  }

  if (
    normalized.includes("429") ||
    normalized.includes(
      "rate limit",
    )
  ) {
    return {
      title: "Rate limited",
      description:
        "Too many requests were sent. Try again shortly.",
      statusCode,
    };
  }

  if (
    normalized.includes("401") ||
    normalized.includes("403") ||
    normalized.includes(
      "invalid api key",
    ) ||
    normalized.includes(
      "unauthorized",
    ) ||
    normalized.includes(
      "authentication",
    )
  ) {
    return {
      title: "Invalid API key",
      description:
        "Check this provider's API key and permissions.",
      statusCode,
    };
  }

  if (
    normalized.includes("404") ||
    normalized.includes(
      "model not found",
    ) ||
    normalized.includes(
      "invalid model",
    ) ||
    normalized.includes(
      "unknown model",
    )
  ) {
    return {
      title: "Invalid model",
      description:
        "The configured model does not exist or is unavailable.",
      statusCode,
    };
  }

  if (
    normalized.includes(
      "connection refused",
    ) ||
    normalized.includes(
      "failed to connect",
    ) ||
    normalized.includes("network") ||
    normalized.includes("timeout") ||
    normalized.includes(
      "timed out",
    )
  ) {
    return {
      title: "Provider offline",
      description:
        "The provider could not be reached.",
      statusCode,
    };
  }

  return {
    title: "Request failed",
    description:
      "The provider returned an unexpected error.",
    statusCode,
  };
}

export default function ProviderErrorCard({
  error,
}: ProviderErrorCardProps) {
  const summary =
    summarizeError(error);

  return (
    <div className="provider-error-card">
      <div className="provider-error-summary">
        <span className="provider-error-icon">
          !
        </span>

        <div>
          <strong>
            {summary.title}
          </strong>

          <p>
            {summary.description}
          </p>
        </div>

        {summary.statusCode && (
          <span className="provider-error-status">
            HTTP {summary.statusCode}
          </span>
        )}
      </div>

      <details className="provider-error-details">
        <summary>
          Show technical details
        </summary>

        <pre>
          <code>
            {error}
          </code>
        </pre>
      </details>
    </div>
  );
}
