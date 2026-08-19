import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Wordcloud } from "@visx/wordcloud";
import { invokeCommand } from "@/lib/commands";
import type { I18nKey } from "@/lib/i18n";

type WordCloudSource = "access" | "cleanup";
type WordCloudRange = "last7_days" | "last30_days" | "all_time";

interface WordCloudTerm {
  text: string;
  score: number;
  occurrences: number;
  recent_occurrences: number;
  frequent_occurrences: number;
  cleanup_occurrences: number;
  total_access_count: number;
  manual_cleanups: number;
  automatic_cleanups: number;
  pinned_items: number;
  blacklist_hits: number;
}

interface WordCloudWarning {
  code: string;
  message: string;
}

interface WordCloudResult {
  source: WordCloudSource;
  range: WordCloudRange | null;
  corpus_items: number;
  truncated: boolean;
  blacklist_terms_included: boolean;
  blacklist_terms_available: boolean;
  terms: WordCloudTerm[];
  warnings: WordCloudWarning[];
}

interface WordDatum extends WordCloudTerm {
  value: number;
}

interface PositionedWord {
  font?: string;
  size?: number;
  text?: string;
  rotate?: number;
  weight?: number | string;
  x?: number;
  y?: number;
}

interface WordCloudPanelProps {
  blacklistTermsEnabled: boolean;
  dataRange: WordCloudRange;
  dataSource: "recent_access" | "recent_cleanup";
  deepParseEnabled: boolean;
  refreshKey: number;
  t: Translate;
}

type Translate = (
  key: I18nKey,
  values?: Record<string, string | number>,
) => string;

const HEIGHT = 360;
const FONT_STACK =
  "Inter Variable, Microsoft YaHei UI, Microsoft JhengHei UI, Segoe UI, sans-serif";

export function WordCloudPanel({
  blacklistTermsEnabled,
  dataRange,
  dataSource,
  deepParseEnabled,
  refreshKey,
  t,
}: WordCloudPanelProps) {
  const source: WordCloudSource =
    dataSource === "recent_cleanup" ? "cleanup" : "access";
  const [result, setResult] = useState<WordCloudResult | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [width, setWidth] = useState(0);
  const [focusedTerm, setFocusedTerm] = useState<{
    term: WordCloudTerm;
    x: number;
    y: number;
  } | null>(null);
  const panelRef = useRef<HTMLDivElement | null>(null);
  const requestId = useRef(0);
  const cache = useRef(new Map<string, WordCloudResult>());

  useEffect(() => {
    const panel = panelRef.current;
    if (!panel) {
      return;
    }
    let frame = 0;
    const updateWidth = () => {
      window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(() => {
        const nextWidth = Math.max(0, Math.floor(panel.clientWidth));
        setWidth((currentWidth) =>
          currentWidth === nextWidth ? currentWidth : nextWidth,
        );
      });
    };
    updateWidth();
    const observer = new ResizeObserver(updateWidth);
    observer.observe(panel);
    return () => {
      window.cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, []);

  useEffect(() => {
    cache.current.clear();
  }, [blacklistTermsEnabled, refreshKey]);

  const load = useCallback(async () => {
    const currentRequest = ++requestId.current;
    const key = `${source}:${dataRange}:${deepParseEnabled}:${blacklistTermsEnabled}:${refreshKey}`;
    const cached = cache.current.get(key);
    if (cached) {
      setResult(cached);
      setError(null);
      setLoading(false);
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const loaded = await invokeCommand<WordCloudResult>("get_dashboard_word_cloud", {
        source,
        range: dataRange,
        deepParseEnabled,
        blacklistTermsEnabled,
      });
      if (currentRequest !== requestId.current) {
        return;
      }
      cache.current.set(key, loaded);
      setResult(loaded);
    } catch (value) {
      if (currentRequest === requestId.current) {
        setError(errorMessage(value));
      }
    } finally {
      if (currentRequest === requestId.current) {
        setLoading(false);
      }
    }
  }, [blacklistTermsEnabled, dataRange, deepParseEnabled, refreshKey, source]);

  useEffect(() => {
    void load();
  }, [load]);

  const terms = result?.terms ?? [];
  const termByText = useMemo(
    () => new Map(terms.map((term) => [term.text, term])),
    [terms],
  );
  const maxScore = useMemo(
    () => Math.max(...terms.map((term) => term.score), 0),
    [terms],
  );
  const minScore = useMemo(
    () => Math.min(...terms.map((term) => term.score), maxScore),
    [maxScore, terms],
  );
  const words = useMemo<WordDatum[]>(
    () =>
      terms.map((term) => ({
        ...term,
        value: term.score,
      })),
    [terms],
  );
  const cloudWidth = Math.max(0, width - 16);
  const seed = useMemo(
    () =>
      terms.reduce(
        (hash, term) => hash ^ hashString(`${term.text}:${term.score}`),
        0x9747b28c ^ cloudWidth,
      ) >>> 0,
    [cloudWidth, terms],
  );
  const random = useMemo(
    () => createLayoutRandom(seed, words.length),
    [seed, words],
  );
  const fontSize = useCallback(
    (word: WordDatum) => {
      if (maxScore === minScore) {
        return 30;
      }
      const normalized = (word.value - minScore) / (maxScore - minScore);
      return 14 + Math.sqrt(Math.max(0, normalized)) * 32;
    },
    [maxScore, minScore],
  );
  const fontWeight = useCallback(
    (word: WordDatum) => (word.blacklist_hits > 0 ? 700 : 500),
    [],
  );

  const showTerms = cloudWidth > 0 && terms.length > 0;
  const warning = result?.warnings.find(
    (item) => item.code === "blacklist_terms_unavailable",
  );
  const partialAccessWarning = result?.warnings.some(
    (item) => item.code === "quick_access_section_unavailable",
  );

  return (
    <section className="min-w-0 bg-card text-card-foreground">
      <div aria-live="polite" aria-busy={loading} className="relative" ref={panelRef}>
        {showTerms ? (
          <div className="relative h-[360px] overflow-hidden px-2" onMouseLeave={() => setFocusedTerm(null)}>
            <Wordcloud
              font={FONT_STACK}
              fontSize={fontSize}
              fontWeight={fontWeight}
              height={HEIGHT}
              padding={4}
              random={random}
              rotate={0}
              spiral="archimedean"
              width={cloudWidth}
              words={words}
            >
              {(cloudWords: PositionedWord[]) =>
                cloudWords.map((cloudWord, index) => {
                  const text = cloudWord.text ?? "";
                  const term = termByText.get(text);
                  if (!term) {
                    return null;
                  }
                  const x = (cloudWord.x ?? 0) + (width - 16) / 2;
                  const y = (cloudWord.y ?? 0) + HEIGHT / 2;
                  return (
                    <text
                      aria-label={termLabel(term, t)}
                      className="cursor-default outline-none"
                      fill={termColor(term, result?.source ?? source)}
                      fontFamily={cloudWord.font}
                      fontSize={cloudWord.size}
                      fontWeight={cloudWord.weight}
                      key={`${text}-${index}`}
                      onFocus={() => setFocusedTerm({ term, x, y })}
                      onMouseEnter={() => setFocusedTerm({ term, x, y })}
                      onMouseMove={() => setFocusedTerm({ term, x, y })}
                      onBlur={() => setFocusedTerm(null)}
                      role="img"
                      tabIndex={0}
                      textAnchor="middle"
                      transform={`translate(${cloudWord.x ?? 0},${cloudWord.y ?? 0}) rotate(${cloudWord.rotate ?? 0})`}
                    >
                      {text}
                      <title>{termLabel(term, t)}</title>
                    </text>
                  );
                })
              }
            </Wordcloud>
            {focusedTerm ? (
              <div
                className="pointer-events-none absolute z-10 max-w-64 rounded-md bg-foreground px-3 py-2 text-xs text-background shadow-md"
                style={{ left: Math.min(Math.max(focusedTerm.x, 8), Math.max(8, width - 180)), top: Math.min(Math.max(focusedTerm.y - 12, 8), HEIGHT - 64) }}
              >
                {termLabel(focusedTerm.term, t)}
              </div>
            ) : null}
            <ol className="sr-only">
              {terms.map((term) => (
                <li key={term.text}>{termLabel(term, t)}</li>
              ))}
            </ol>
          </div>
        ) : (
          <div className="grid h-[360px] place-items-center px-4 text-sm text-muted-foreground">
            {loading ? t("wordCloudLoading") : error ?? t("wordCloudNoTerms")}
          </div>
        )}
        {error && showTerms ? (
          <p className="absolute bottom-2 left-3 right-3 text-xs text-destructive" role="status">
            {error}
          </p>
        ) : null}
      </div>
      {result && result.truncated ? (
        <p className="border-t px-4 py-2 text-xs text-muted-foreground" role="status">
          {t("wordCloudSampleTruncated", { count: result.corpus_items })}
        </p>
      ) : null}
      {warning || (result && !result.blacklist_terms_available && blacklistTermsEnabled) ? (
        <p className="border-t px-4 py-2 text-xs text-muted-foreground" role="status">
          {t("wordCloudBlacklistUnavailable")}
        </p>
      ) : null}
      {partialAccessWarning ? (
        <p className="border-t px-4 py-2 text-xs text-muted-foreground" role="status">
          {t("wordCloudPartialAccessUnavailable")}
        </p>
      ) : null}
    </section>
  );
}

function termColor(term: WordCloudTerm, source: WordCloudSource) {
  if (term.blacklist_hits > 0) {
    return "var(--destructive)";
  }
  if (source === "cleanup") {
    return "var(--chart-2)";
  }
  if (term.recent_occurrences > term.frequent_occurrences) {
    return "var(--chart-1)";
  }
  if (term.frequent_occurrences > term.recent_occurrences) {
    return "var(--chart-2)";
  }
  return "var(--foreground)";
}

function termLabel(term: WordCloudTerm, t: Translate) {
  const parts = [
    `${term.text}: ${t("wordCloudOccurrences", { count: term.occurrences })}`,
    t("wordCloudScore", { score: term.score.toFixed(2) }),
  ];
  if (term.recent_occurrences || term.frequent_occurrences) {
    parts.push(
      t("wordCloudAccessCount", {
        count: term.total_access_count,
        recent: term.recent_occurrences,
        frequent: term.frequent_occurrences,
      }),
    );
  }
  if (term.cleanup_occurrences) {
    parts.push(
      t("wordCloudCleanupCount", {
        count: term.cleanup_occurrences,
        manual: term.manual_cleanups,
        automatic: term.automatic_cleanups,
      }),
    );
  }
  if (term.blacklist_hits) {
    parts.push(t("wordCloudBlacklistHits", { count: term.blacklist_hits }));
  }
  return parts.join(" · ");
}

function hashString(value: string) {
  let hash = 2166136261;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

function createLayoutRandom(seed: number, wordCount: number) {
  // d3-cloud 1.2.x consumes three random values per word with rotate={0}:
  // initial x, initial y, and the spiral direction. Cycling at that boundary
  // makes React StrictMode's effect replay start from the same seeded state.
  const callsPerLayout = Math.max(1, wordCount * 3);
  let calls = 0;
  let random = mulberry32(seed);

  return () => {
    if (calls >= callsPerLayout) {
      calls = 0;
      random = mulberry32(seed);
    }
    calls += 1;
    return random();
  };
}

function mulberry32(seed: number) {
  let state = seed;
  return () => {
    state += 0x6d2b79f5;
    let value = state;
    value = Math.imul(value ^ (value >>> 15), value | 1);
    value ^= value + Math.imul(value ^ (value >>> 7), value | 61);
    return ((value ^ (value >>> 14)) >>> 0) / 4294967296;
  };
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
