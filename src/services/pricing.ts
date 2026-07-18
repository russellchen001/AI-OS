import {
  MODEL_PRICING,
  MODEL_PRICE_ALIASES,
  type ModelPrice,
} from "../config/modelPricing";

export type CalculateCostInput = {
  provider?: string;
  model?: string;
  inputTokens?: number;
  outputTokens?: number;
};

export type CostEstimate = {
  cost: number;
  matched: boolean;
  provider: string;
  model: string;
  inputPerMillion: number;
  outputPerMillion: number;
};

function normalize(
  value?: string,
): string {
  return (
    value
      ?.trim()
      .toLowerCase() ?? ""
  );
}

function safeTokens(
  value?: number,
): number {
  return typeof value === "number" &&
    Number.isFinite(value) &&
    value > 0
    ? value
    : 0;
}

function findPrice(
  provider: string,
  model: string,
): ModelPrice | undefined {
  const originalKey =
    `${provider}:${model}`;

  const aliasedKey =
    MODEL_PRICE_ALIASES[
      originalKey
    ] ?? originalKey;

  const separator =
    aliasedKey.indexOf(":");

  const resolvedProvider =
    separator >= 0
      ? aliasedKey.slice(
          0,
          separator,
        )
      : provider;

  const resolvedModel =
    separator >= 0
      ? aliasedKey.slice(
          separator + 1,
        )
      : model;

  return (
    MODEL_PRICING.find(
      (price) =>
        normalize(
          price.provider,
        ) ===
          resolvedProvider &&
        normalize(
          price.model,
        ) ===
          resolvedModel,
    ) ??
    MODEL_PRICING.find(
      (price) =>
        normalize(
          price.provider,
        ) ===
          resolvedProvider &&
        price.model === "*",
    )
  );
}

export function calculateCost({
  provider,
  model,
  inputTokens,
  outputTokens,
}: CalculateCostInput):
  CostEstimate {
  const normalizedProvider =
    normalize(provider);

  const normalizedModel =
    normalize(model);

  const price =
    findPrice(
      normalizedProvider,
      normalizedModel,
    );

  if (!price) {
    return {
      cost: 0,
      matched: false,
      provider:
        normalizedProvider,
      model:
        normalizedModel,
      inputPerMillion: 0,
      outputPerMillion: 0,
    };
  }

  const inputCost =
    (
      safeTokens(
        inputTokens,
      ) /
      1_000_000
    ) *
    price.inputPerMillion;

  const outputCost =
    (
      safeTokens(
        outputTokens,
      ) /
      1_000_000
    ) *
    price.outputPerMillion;

  return {
    cost:
      inputCost +
      outputCost,
    matched: true,
    provider:
      normalizedProvider,
    model:
      normalizedModel,
    inputPerMillion:
      price.inputPerMillion,
    outputPerMillion:
      price.outputPerMillion,
  };
}
