import type { SpeedTestResult } from "../types/speedtest";
import type { BatchSummary, ScatterPoint } from "../types/history";
import { invokeTauri } from "./helpers";

export async function dbSaveBatch(
  subscriptionText: string,
  configJson: string,
  results: SpeedTestResult[]
): Promise<number> {
  return invokeTauri<number>("db_save_batch", {
    subscriptionText,
    configJson,
    results,
  });
}

export async function dbGetBatches(
  fromTimestamp?: number,
  toTimestamp?: number,
  limit?: number,
  offset?: number
): Promise<BatchSummary[]> {
  return invokeTauri<BatchSummary[]>("db_get_batches", {
    fromTimestamp,
    toTimestamp,
    limit,
    offset,
  });
}

export async function dbGetBatchResults(batchId: number): Promise<SpeedTestResult[]> {
  return invokeTauri<SpeedTestResult[]>("db_get_batch_results", { batchId });
}

export async function dbDeleteBatches(batchIds: number[]): Promise<number> {
  return invokeTauri<number>("db_delete_batches", { batchIds });
}

export async function dbDeleteBatchesOlderThan(months: number): Promise<number> {
  return invokeTauri<number>("db_delete_batches_older_than", { months });
}

export async function dbClearAllBatches(): Promise<number> {
  return invokeTauri<number>("db_clear_all_batches");
}

export async function dbGetScatterData(
  fromTimestamp?: number,
  toTimestamp?: number
): Promise<ScatterPoint[]> {
  return invokeTauri<ScatterPoint[]>("db_get_scatter_data", {
    fromTimestamp,
    toTimestamp,
  });
}

export async function dbGetAllCountries(): Promise<string[]> {
  return invokeTauri<string[]>("db_get_all_countries");
}
