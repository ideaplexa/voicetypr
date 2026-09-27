export const TRANSCRIPTION_STREAM_EVENT = "transcription-stream" as const;

export interface EngineStreamCapabilities {
  supports_streaming: boolean;
  supports_committed_prefix: boolean;
  supports_tentative_tail: boolean;
  supports_endpointing: boolean;
  final_only: boolean;
}

export type TranscriptionStreamEvent =
  | {
      type: "started";
      session_id: number;
      engine: string;
      revision: number;
    }
  | {
      type: "partial";
      session_id: number;
      revision: number;
      committed: string;
      tentative: string;
    }
  | {
      type: "final";
      session_id: number;
      revision: number;
      text: string;
    }
  | {
      type: "cancelled";
      session_id: number;
      revision: number;
    }
  | {
      type: "error";
      session_id: number;
      revision: number;
      error: string;
    };
