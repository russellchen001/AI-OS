import type {
  ArenaContextEntry,
  ArenaParticipantRuntime,
  ArenaVisibility,
} from "../types/arena";

export class ArenaContextStore {
  private readonly entries: ArenaContextEntry[] = [];

  add(input: Omit<ArenaContextEntry, "id" | "createdAt">): ArenaContextEntry {
    const entry: ArenaContextEntry = {
      ...input,
      id: crypto.randomUUID(),
      createdAt: Date.now(),
    };

    this.entries.push(entry);
    return entry;
  }

  all(): ArenaContextEntry[] {
    return this.entries.map((entry) => ({
      ...entry,
      visibility: { ...entry.visibility } as ArenaVisibility,
      tags: entry.tags ? [...entry.tags] : undefined,
    }));
  }

  projectForParticipant(
    participant: ArenaParticipantRuntime,
  ): ArenaContextEntry[] {
    return this.entries.filter((entry) => {
      switch (entry.visibility.kind) {
        case "public":
          return true;

        case "participant-private":
          return entry.visibility.participantId === participant.seatId;

        case "team-private":
          return Boolean(
            participant.team &&
              entry.visibility.teamId === participant.team,
          );

        case "runtime-private":
          return false;
      }
    });
  }

  projectForRuntime(): ArenaContextEntry[] {
    return this.all();
  }

  renderForParticipant(
    participant: ArenaParticipantRuntime,
  ): string {
    return this.projectForParticipant(participant)
      .map((entry) => {
        const prefix =
          entry.visibility.kind === "public"
            ? "PUBLIC"
            : entry.visibility.kind === "participant-private"
              ? "PRIVATE"
              : "TEAM";

        return `[${prefix}] ${entry.content}`;
      })
      .join("\n");
  }
}
