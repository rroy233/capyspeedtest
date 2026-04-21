import type { NodeInfo } from "../types/speedtest";

interface RegexCompileResult {
  regex: RegExp | null;
  error: string | null;
}

interface NodeRegexFilterResult {
  nodes: NodeInfo[];
  error: string | null;
}

function isEscaped(text: string, index: number): boolean {
  let slashCount = 0;
  for (let i = index - 1; i >= 0 && text[i] === "\\"; i -= 1) {
    slashCount += 1;
  }
  return slashCount % 2 === 1;
}

function parseRegexLiteral(input: string): { pattern: string; flags: string } | null {
  if (!input.startsWith("/") || input.length < 2) {
    return null;
  }
  for (let i = input.length - 1; i > 0; i -= 1) {
    if (input[i] === "/" && !isEscaped(input, i)) {
      return {
        pattern: input.slice(1, i),
        flags: input.slice(i + 1),
      };
    }
  }
  return null;
}

export function compileUserRegex(input: string): RegexCompileResult {
  const source = input.trim();
  if (!source) {
    return { regex: null, error: null };
  }

  const literal = parseRegexLiteral(source);
  try {
    if (literal) {
      return { regex: new RegExp(literal.pattern, literal.flags), error: null };
    }
    return { regex: new RegExp(source), error: null };
  } catch (error) {
    const message = error instanceof Error ? error.message : "未知错误";
    return { regex: null, error: `无效正则表达式: ${message}` };
  }
}

function testName(regex: RegExp, name: string): boolean {
  if (regex.global || regex.sticky) {
    regex.lastIndex = 0;
  }
  return regex.test(name);
}

export function filterNodesByNameRegex(nodes: NodeInfo[], input: string): NodeRegexFilterResult {
  const { regex, error } = compileUserRegex(input);
  if (error) {
    return { nodes: [], error };
  }
  if (!regex) {
    return { nodes, error: null };
  }
  return {
    nodes: nodes.filter((node) => testName(regex, node.name)),
    error: null,
  };
}
