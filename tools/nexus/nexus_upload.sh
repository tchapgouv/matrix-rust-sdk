#!/usr/bin/env bash
NEXUS_BASE_URL=$1
FILTER=$2
EXT=$3
VERSION=$4
DATE_STR=$5

check_last_exit_code () {
    if [ $1 -ne 0 ]; then
        echo "Error: exit code != 0"
        exit $1
    fi
}

echo "checking if files found"
find . -name "*$FILTER*.$EXT" | grep .
check_last_exit_code $?
echo "find end"

find . -name "*$FILTER*.$EXT" -print0 | while read -d $'\0' file
do
  BASENAME=${file##*/}
  echo "Source: $file"
  echo "Destination $NEXUS_BASE_URL/$VERSION/$BASENAME"
  #curl --fail -v -u $S_BWMESSENGER_ID:$S_BWMESSENGER_PASSWORD --upload-file $file $NEXUS_BASE_URL/$VERSION/$BASENAME
  echo "uploading " + $S_BWMESSENGER_ID:$S_BWMESSENGER_PASSWORD + "--upload-file" $file $NEXUS_BASE_URL/$VERSION/$BASENAME
  check_last_exit_code $?
done
check_last_exit_code $?
