# Java benchmark subjects

Measures NetworkTables 4, the original Java TARWYN, and a plain UDP client
against the same wire format the Rust harness uses, so results are directly
comparable.

Requires a JDK. No Gradle or Maven — `javac` is enough.

## Dependencies

Downloaded rather than vendored, since none of them belong in this repository.

    JARS=/path/to/jars
    mkdir -p "$JARS" && cd "$JARS"

    WPI=https://frcmaven.wpi.edu/artifactory/release/edu/wpi/first
    V=2025.3.2
    curl -sL -o ntcore-java.jar   "$WPI/ntcore/ntcore-java/$V/ntcore-java-$V.jar"
    curl -sL -o wpiutil-java.jar  "$WPI/wpiutil/wpiutil-java/$V/wpiutil-java-$V.jar"
    curl -sL -o ntcore-jni.jar    "$WPI/ntcore/ntcore-jni/$V/ntcore-jni-$V-linuxx86-64.jar"
    curl -sL -o wpiutil-jni.jar   "$WPI/wpiutil/wpiutil-jni/$V/wpiutil-jni-$V-linuxx86-64.jar"

    MC=https://repo1.maven.org/maven2/com/fasterxml/jackson/core
    J=2.15.2
    for a in core databind annotations; do
      curl -sL -o "jackson-$a.jar" "$MC/jackson-$a/$J/jackson-$a-$J.jar"
    done

    gh release download v5.0.0 -R Team488/tarwyn -p TARWYN.jar

ntcore-jni is published only up to 2025.3.2, so ntcore-java is pinned to the
same version rather than 2026.x. wpiutil needs Jackson at runtime and does not
bundle it.

Extract the native libraries:

    mkdir -p "$JARS/natives" && cd "$JARS/natives"
    for j in ../ntcore-jni.jar ../wpiutil-jni.jar; do
      python3 -c "import zipfile,sys,os
    z=zipfile.ZipFile(sys.argv[1])
    for n in z.namelist():
        if n.endswith('.so') and 'debug' not in n:
            open(os.path.basename(n),'wb').write(z.read(n))" "$j"
    done

## Build

    javac -cp "$(ls $JARS/*.jar | tr '\n' ':')" -d out src/*.java

## Run

    CP="out:$(ls $JARS/*.jar | tr '\n' ':')"

NetworkTables 4. `libntcorejni.so` resolves symbols against `libwpiutiljni.so`,
which the JVM does not load first on its own, so preload it:

    export LD_PRELOAD="$JARS/natives/libwpiutiljni.so"
    java --enable-native-access=ALL-UNNAMED -Djava.library.path="$JARS/natives" \
      -cp "$CP" Bench subscriber --subject nt4 --port 48820 --payload 96 --samples 3000 &
    java --enable-native-access=ALL-UNNAMED -Djava.library.path="$JARS/natives" \
      -cp "$CP" Bench publisher --subject nt4 --port 48820 --payload 96 --rate 1000 --count 6000

The subscriber runs the NT server and the publisher connects as a client, which
matches the robot and coprocessor split rather than reversing it.

Plain UDP, no dependencies beyond the JDK:

    java -cp out Bench subscriber --subject java-udp --port 48811 --payload 96 --samples 3000 &
    java -cp out Bench publisher  --subject java-udp --port 48811 --payload 96 --rate 5000 --count 6000

TARWYN, with its own server running first:

    unset LD_PRELOAD
    java -cp "$JARS/TARWYN.jar" org.team488.JServer.Main &
    java -cp "$CP" Bench subscriber --subject tarwyn-java --payload 96 --samples 3000 &
    java -cp "$CP" Bench publisher  --subject tarwyn-java --payload 96 --rate 1000 --count 8000

## Fairness

Any NetworkTables number is meaningless without the configuration that produced
it. `Nt4Subject.configDescription()` prints the options in use alongside every
result, and the subscriber spins rather than sleeping between `readQueue()`
calls so the harness does not charge its own polling interval to NT.

Results are in ../RESULTS.md.
