# Java benchmark subjects

NetworkTables 4 and the original Java TARWYN, measured against the same wire
format the Rust harness uses. Needs a JDK; no Gradle or Maven.

## Jars

    JARS=/path/to/jars && mkdir -p "$JARS" && cd "$JARS"

    WPI=https://frcmaven.wpi.edu/artifactory/release/edu/wpi/first
    V=2025.3.2
    curl -sL -o ntcore-java.jar  "$WPI/ntcore/ntcore-java/$V/ntcore-java-$V.jar"
    curl -sL -o wpiutil-java.jar "$WPI/wpiutil/wpiutil-java/$V/wpiutil-java-$V.jar"
    curl -sL -o ntcore-jni.jar   "$WPI/ntcore/ntcore-jni/$V/ntcore-jni-$V-linuxx86-64.jar"
    curl -sL -o wpiutil-jni.jar  "$WPI/wpiutil/wpiutil-jni/$V/wpiutil-jni-$V-linuxx86-64.jar"

    MC=https://repo1.maven.org/maven2/com/fasterxml/jackson/core
    for a in core databind annotations; do
      curl -sL -o "jackson-$a.jar" "$MC/jackson-$a/2.15.2/jackson-$a-2.15.2.jar"
    done

    gh release download v5.0.0 -R Team488/tarwyn -p TARWYN.jar

    mkdir -p natives && cd natives
    for j in ../ntcore-jni.jar ../wpiutil-jni.jar; do
      python3 -c "import zipfile,sys,os
    z=zipfile.ZipFile(sys.argv[1])
    for n in z.namelist():
        if n.endswith('.so') and 'debug' not in n:
            open(os.path.basename(n),'wb').write(z.read(n))" "$j"
    done

Two things that are not obvious: ntcore-jni is published only up to 2025.3.2, so
ntcore-java is pinned to match rather than 2026.x; and wpiutil needs Jackson at
runtime without bundling it.

## Build and run

    javac -cp "$(ls $JARS/*.jar | tr '\n' ':')" -d out src/*.java

Normally driven by [../generate.sh](../generate.sh). Directly:

    CP="out:$(ls $JARS/*.jar | tr '\n' ':')"

    export LD_PRELOAD="$JARS/natives/libwpiutiljni.so"
    java --enable-native-access=ALL-UNNAMED -Djava.library.path="$JARS/natives" \
      -cp "$CP" Bench subscriber --subject nt4 --port 48820 --payload 96 --samples 3000 &
    java --enable-native-access=ALL-UNNAMED -Djava.library.path="$JARS/natives" \
      -cp "$CP" Bench publisher --subject nt4 --port 48820 --payload 96 --rate 500 --count 8000

`libntcorejni.so` resolves symbols against `libwpiutiljni.so`, which the JVM does
not load first on its own, hence the preload. The subscriber runs the NT server
and the publisher connects as a client, matching the robot and coprocessor split.

TARWYN needs its own server running first:

    unset LD_PRELOAD
    java -cp "$JARS/TARWYN.jar" org.team488.JServer.Main &
    java -cp "$CP" Bench subscriber --subject tarwyn-java --payload 96 --samples 3000 &
    java -cp "$CP" Bench publisher  --subject tarwyn-java --payload 96 --rate 500 --count 8000
