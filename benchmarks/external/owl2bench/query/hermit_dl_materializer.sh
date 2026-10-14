#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOURCE_IMPL="$SCRIPT_DIR/HermitDlMaterializer.java"
NOT_AVAILABLE_EXIT_CODE=125

usage() {
  echo "usage: $0 source <ontology.rdf> | version <ontology.rdf> | export <source.nq>" >&2
  exit 2
}

not_available() {
  printf '[hermit-dl-materializer][not_available] %s\n' "$1" >&2
  exit "$NOT_AVAILABLE_EXIT_CODE"
}

materializer_failed() {
  printf '[hermit-dl-materializer][failed] %s\n' "$1" >&2
  exit 1
}

require_file() {
  local path="$1"
  local label="$2"
  [[ -n "$path" && -r "$path" && -f "$path" ]] \
    || not_available "$label is missing or unreadable: $path"
  realpath "$path" || not_available "cannot resolve $label: $path"
}

require_directory() {
  local path="$1"
  local label="$2"
  [[ -n "$path" && -r "$path" && -d "$path" ]] \
    || not_available "$label is missing or unreadable: $path"
  realpath "$path" || not_available "cannot resolve $label: $path"
}

artifact_hermit_version() {
  command -v unzip >/dev/null 2>&1 \
    || not_available "unzip is required to inspect HermiT artifact metadata"
  unzip -p "$HERMIT_DL_JAR_PATH" \
    META-INF/maven/com.hermit-reasoner/org.semanticweb.hermit/pom.properties 2>/dev/null \
    | awk -F= '$1 == "version" { print $2; exit }'
}

classpath_fingerprint() {
  local classpath="$1"
  local entry directory path fingerprint=""
  local -a entries=() jars=()
  IFS=':' read -r -a entries <<< "$classpath"
  for entry in "${entries[@]}"; do
    [[ -n "$entry" ]] || not_available "HermiT classpath contains an empty entry"
    if [[ "$entry" == *"*" ]]; then
      directory="${entry%\*}"
      [[ -d "$directory" ]] || not_available "HermiT classpath directory is missing: $directory"
      mapfile -t jars < <(find "$directory" -maxdepth 1 -type f -name '*.jar' -print | sort)
      ((${#jars[@]} > 0)) || not_available "HermiT classpath directory contains no jars: $directory"
      for path in "${jars[@]}"; do
        fingerprint+="$path\t$(sha256sum "$path" | awk '{print $1}')\n"
      done
    else
      [[ -f "$entry" && -r "$entry" ]] || not_available "HermiT classpath artifact is missing: $entry"
      fingerprint+="$entry\t$(sha256sum "$entry" | awk '{print $1}')\n"
    fi
  done
  [[ -n "$fingerprint" ]] || not_available "HermiT classpath contains no artifacts"
  printf '%b' "$fingerprint" | sha256sum | awk '{print $1}'
}

prepare_runtime() {
  HERMIT_DL_JAR_PATH="$(require_file "${HERMIT_DL_JAR:-}" 'HermiT jar')"
  HERMIT_DL_CONFIG_PATH="$(require_file "${HERMIT_DL_CONFIG:-}" 'HermiT configuration')"
  local configured_version artifact_version
  configured_version="$(awk -F= '$1 == "hermit.version" { print $2; exit }' "$HERMIT_DL_CONFIG_PATH")"
  [[ -n "$configured_version" ]] || not_available "HermiT configuration pin hermit.version is missing"
  artifact_version="$(artifact_hermit_version)"
  [[ -n "$artifact_version" ]] || not_available "HermiT artifact version metadata is missing"
  [[ "$configured_version" == "$artifact_version" ]] \
    || materializer_failed "HermiT configuration version $configured_version does not match artifact metadata $artifact_version"
  [[ -r "$SOURCE_IMPL" ]] || materializer_failed "materializer source is missing: $SOURCE_IMPL"
  command -v java >/dev/null 2>&1 || not_available "java runtime is missing"
  command -v javac >/dev/null 2>&1 || not_available "javac compiler is missing"

  HERMIT_DL_CLASSPATH_VALUE="${HERMIT_DL_CLASSPATH:-$HERMIT_DL_JAR_PATH}"
  if [[ -n "${HERMIT_DL_BUILD_DIR:-}" ]]; then
    HERMIT_DL_BUILD_PATH="$HERMIT_DL_BUILD_DIR"
  else
    HERMIT_DL_BUILD_PATH="$(mktemp -d "${TMPDIR:-/tmp}/tableauxx-hermit-dl-materializer.XXXXXX")" \
      || materializer_failed "cannot create a private HermiT build directory"
  fi
  mkdir -p "$HERMIT_DL_BUILD_PATH" \
    || materializer_failed "cannot create build directory: $HERMIT_DL_BUILD_PATH"
  chmod 700 "$HERMIT_DL_BUILD_PATH" \
    || materializer_failed "cannot restrict build directory permissions: $HERMIT_DL_BUILD_PATH"
  local source_hash jar_hash classpath_hash marker expected
  source_hash="$(sha256sum "$SOURCE_IMPL" | awk '{print $1}')"
  jar_hash="$(sha256sum "$HERMIT_DL_JAR_PATH" | awk '{print $1}')"
  classpath_hash="$(classpath_fingerprint "$HERMIT_DL_CLASSPATH_VALUE")"
  marker="$HERMIT_DL_BUILD_PATH/.materializer-inputs"
  expected="$source_hash $jar_hash $classpath_hash $HERMIT_DL_CLASSPATH_VALUE"
  if [[ ! -s "$HERMIT_DL_BUILD_PATH/HermitDlMaterializer.class" \
      || ! -r "$marker" || "$(<"$marker")" != "$expected" ]]; then
    javac -cp "$HERMIT_DL_CLASSPATH_VALUE" \
      -d "$HERMIT_DL_BUILD_PATH" "$SOURCE_IMPL" \
      || materializer_failed "javac could not compile the HermiT materializer"
    printf '%s\n' "$expected" > "$marker"
  fi
}

run_java() {
  local -a java_options=()
  if [[ -n "${HERMIT_DL_JAVA_OPTS:-}" ]]; then
    read -r -a java_options <<< "$HERMIT_DL_JAVA_OPTS"
  fi
  java "${java_options[@]}" \
    -cp "$HERMIT_DL_BUILD_PATH:$HERMIT_DL_CLASSPATH_VALUE" \
    HermitDlMaterializer "$@"
}

[[ $# -ge 1 ]] || usage
prepare_runtime

case "$1" in
  source)
    [[ $# -eq 2 ]] || usage
    ontology="$(require_file "$2" 'source ontology')"
    run_java source "$ontology"
    ;;
  version)
    [[ $# -eq 2 ]] || usage
    ontology="$(require_file "$2" 'source ontology')"
    run_java version "$ontology" "$HERMIT_DL_CONFIG_PATH"
    ;;
  export)
    [[ $# -eq 2 ]] || usage
    source_graph="$(require_file "$2" 'source graph')"
    ontology="$(require_file "${HERMIT_DL_SOURCE_ONTOLOGY:-}" 'source ontology')"
    query_directory="$(require_directory "${HERMIT_DL_QUERY_DIR:-}" 'query directory')"
    run_java export "$ontology" "$source_graph" "$query_directory" "$HERMIT_DL_CONFIG_PATH"
    ;;
  *)
    usage
    ;;
esac
