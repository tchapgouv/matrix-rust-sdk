const { OlmMachine, UserId, DeviceId, DeviceKeyId, RoomId, DeviceKeyAlgorithName, Device, LocalTrust, UserDevices, DeviceKey, DeviceKeyName, DeviceKeyAlgorithmName, Ed25519PublicKey, Curve25519PublicKey, Signatures, VerificationMethod, VerificationRequest, ToDeviceRequest, DeviceLists, KeysUploadRequest, RequestType, KeysQueryRequest, Sas, Emoji } = require('../pkg/matrix_sdk_crypto_js');
const { zip, addMachineToMachine } = require('./helper');

describe('LocalTrust', () => {
    test('has the correct variant values', () => {
        expect(LocalTrust.Verified).toStrictEqual(0);
        expect(LocalTrust.BlackListed).toStrictEqual(1);
        expect(LocalTrust.Ignored).toStrictEqual(2);
        expect(LocalTrust.Unset).toStrictEqual(3);
    });
});

describe('DeviceKeyName', () => {
    test('has the correct variant values', () => {
        expect(DeviceKeyName.Curve25519).toStrictEqual(0);
        expect(DeviceKeyName.Ed25519).toStrictEqual(1);
        expect(DeviceKeyName.Unknown).toStrictEqual(2);
    });
});

describe(OlmMachine.name, () => {
    const user = new UserId('@alice:example.org');
    const device = new DeviceId('foobar');
    const room = new RoomId('!baz:matrix.org');

    function machine(new_user, new_device) {
        return new OlmMachine(new_user || user, new_device || device);
    }

    test('can read user devices', async () => {
        const m = await machine();
        const userDevices = await m.getUserDevices(user);

        expect(userDevices).toBeInstanceOf(UserDevices);
        expect(userDevices.get(device)).toBeInstanceOf(Device);
        expect(userDevices.isAnyVerified()).toStrictEqual(false);
        expect(userDevices.keys().map(device_id => device_id.toString())).toStrictEqual([device.toString()]);
        expect(userDevices.devices().map(device => device.deviceId.toString())).toStrictEqual([device.toString()]);
    });

    test('can read a user device', async () => {
        const m = await machine();
        const dev = await m.getDevice(user, device);

        expect(dev).toBeInstanceOf(Device);
        expect(dev.isVerified()).toStrictEqual(false);
        expect(dev.isCrossSigningTrusted()).toStrictEqual(false);

        expect(dev.localTrustState).toStrictEqual(LocalTrust.Unset);
        expect(dev.isLocallyTrusted()).toStrictEqual(false);
        expect(await dev.setLocalTrust(LocalTrust.Verified)).toBeNull();
        expect(dev.localTrustState).toStrictEqual(LocalTrust.Verified);
        expect(dev.isLocallyTrusted()).toStrictEqual(true);

        expect(dev.userId.toString()).toStrictEqual(user.toString());
        expect(dev.deviceId.toString()).toStrictEqual(device.toString());
        expect(dev.deviceName).toBeUndefined();

        const deviceKey = dev.getKey(DeviceKeyAlgorithmName.Ed25519);

        expect(deviceKey).toBeInstanceOf(DeviceKey);
        expect(deviceKey.name).toStrictEqual(DeviceKeyName.Ed25519);
        expect(deviceKey.curve25519).toBeUndefined();
        expect(deviceKey.ed25519).toBeInstanceOf(Ed25519PublicKey);
        expect(deviceKey.unknown).toBeUndefined();
        expect(deviceKey.toBase64()).toMatch(/^[A-Za-z0-9\+/]+$/);

        expect(dev.curve25519Key).toBeInstanceOf(Curve25519PublicKey);
        expect(dev.ed25519Key).toBeInstanceOf(Ed25519PublicKey);

        for (const [deviceKeyId, deviceKey] of dev.keys) {
            expect(deviceKeyId).toBeInstanceOf(DeviceKeyId);
            expect(deviceKey).toBeInstanceOf(DeviceKey);
        }

        expect(dev.signatures).toBeInstanceOf(Signatures);
        expect(dev.isBlacklisted()).toStrictEqual(false);
        expect(dev.isDeleted()).toStrictEqual(false);
    });
});

describe('Key Verification', () => {
    const userId1 = new UserId('@alice:example.org');
    const deviceId1 = new DeviceId('alice_device');

    const userId2 = new UserId('@bob:example.org');
    const deviceId2 = new DeviceId('bob_device');

    function machine(new_user, new_device) {
        return new OlmMachine(new_user || userId1, new_device || deviceId1);
    }

    describe('SAS', () => {
        // First Olm machine.
        let m1;

        // Second Olm machine.
        let m2;

        beforeAll(async () => {
            m1 = await machine(userId1, deviceId1);
            m2 = await machine(userId2, deviceId2);
        });

        // Verification request for `m1`.
        let verificationRequest1;

        // The flow ID.
        let flowId;

        test('can request verification (`m.key.verification.request`)', async () => {
            // Make `m1` and `m2` be aware of each other.
            {
                await addMachineToMachine(m2, m1);
                await addMachineToMachine(m1, m2);
            }

            // Pick the device we want to start the verification with.
            const device2 = await m1.getDevice(userId2, deviceId2);

            expect(device2).toBeInstanceOf(Device);

            let outgoingVerificationRequest;
            // Request a verification from `m1` to `device2`.
            [verificationRequest1, outgoingVerificationRequest] = await device2.requestVerification();

            expect(verificationRequest1).toBeInstanceOf(VerificationRequest);

            expect(verificationRequest1.ownUserId.toString()).toStrictEqual(userId1.toString());
            expect(verificationRequest1.otherUserId.toString()).toStrictEqual(userId2.toString());
            expect(verificationRequest1.otherDeviceId).toBeUndefined();
            expect(verificationRequest1.roomId).toBeUndefined();
            expect(verificationRequest1.cancelInfo).toBeUndefined();
            expect(verificationRequest1.isPassive()).toStrictEqual(false);
            expect(verificationRequest1.isReady()).toStrictEqual(false);
            expect(verificationRequest1.timedOut()).toStrictEqual(false);
            expect(verificationRequest1.theirSupportedMethods).toBeUndefined();
            expect(verificationRequest1.ourSupportedMethods).toStrictEqual([VerificationMethod.SasV1, VerificationMethod.ReciprocateV1]);
            expect(verificationRequest1.flowId).toMatch(/^[a-f0-9]+$/);
            expect(verificationRequest1.isSelfVerification()).toStrictEqual(false);
            expect(verificationRequest1.weStarted()).toStrictEqual(true);
            expect(verificationRequest1.isDone()).toStrictEqual(false);
            expect(verificationRequest1.isCancelled()).toStrictEqual(false);

            expect(outgoingVerificationRequest).toBeInstanceOf(ToDeviceRequest);

            outgoingVerificationRequest = JSON.parse(outgoingVerificationRequest.body);
            expect(outgoingVerificationRequest.event_type).toStrictEqual('m.key.verification.request');

            const outgoingContent = outgoingVerificationRequest.messages[userId2.toString()][deviceId2.toString()];

            const toDeviceEvents = {
                events: [{
                    sender: userId1.toString(),
                    type: outgoingVerificationRequest.event_type,
                    content: outgoingContent,
                }]
            };

            // Let's send the verification request to `m2`.
            await m2.receiveSyncChanges(JSON.stringify(toDeviceEvents), new DeviceLists(), new Map(), new Set());

            flowId = outgoingContent.transaction_id;
        });

        // Verification request for `m2`.
        let verificationRequest2;

        test('can fetch received request verification', async () => {
            // Oh, a new verification request.
            verificationRequest2 = m2.getVerificationRequest(userId1, flowId);

            expect(verificationRequest2).toBeInstanceOf(VerificationRequest);

            expect(verificationRequest2.ownUserId.toString()).toStrictEqual(userId2.toString());
            expect(verificationRequest2.otherUserId.toString()).toStrictEqual(userId1.toString());
            expect(verificationRequest2.otherDeviceId.toString()).toStrictEqual(deviceId1.toString());
            expect(verificationRequest2.roomId).toBeUndefined();
            expect(verificationRequest2.cancelInfo).toBeUndefined();
            expect(verificationRequest2.isPassive()).toStrictEqual(false);
            expect(verificationRequest2.isReady()).toStrictEqual(false);
            expect(verificationRequest2.timedOut()).toStrictEqual(false);
            expect(verificationRequest2.theirSupportedMethods).toStrictEqual([VerificationMethod.SasV1, VerificationMethod.ReciprocateV1]);
            expect(verificationRequest2.ourSupportedMethods).toBeUndefined();
            expect(verificationRequest2.flowId).toMatch(/^[a-f0-9]+$/);
            expect(verificationRequest2.isSelfVerification()).toStrictEqual(false);
            expect(verificationRequest2.weStarted()).toStrictEqual(false);
            expect(verificationRequest2.isDone()).toStrictEqual(false);
            expect(verificationRequest2.isCancelled()).toStrictEqual(false);

            const verificationRequests = m2.getVerificationRequests(userId1);
            expect(verificationRequests).toHaveLength(1);
            expect(verificationRequests[0].flowId).toStrictEqual(verificationRequest2.flowId); // there are the same
        });

        test('can accept a verification request (`m.key.verification.ready`)', async () => {
            // Accept the verification request.
            let outgoingVerificationRequest = verificationRequest2.accept();

            expect(outgoingVerificationRequest).toBeInstanceOf(ToDeviceRequest);

            // The request verification is ready.
            outgoingVerificationRequest = JSON.parse(outgoingVerificationRequest.body);
            expect(outgoingVerificationRequest.event_type).toStrictEqual('m.key.verification.ready');

            const toDeviceEvents = {
                events: [{
                    sender: userId2.toString(),
                    type: outgoingVerificationRequest.event_type,
                    content: outgoingVerificationRequest.messages[userId1.toString()][deviceId1.toString()],
                }],
            };

            // Let's send the verification ready to `m1`.
            await m1.receiveSyncChanges(JSON.stringify(toDeviceEvents), new DeviceLists(), new Map(), new Set());
        });

        test('verification requests are synchronized and automatically updated', () => {
            expect(verificationRequest1.isReady()).toStrictEqual(true);
            expect(verificationRequest2.isReady()).toStrictEqual(true);

            expect(verificationRequest1.theirSupportedMethods).toStrictEqual([VerificationMethod.SasV1, VerificationMethod.ReciprocateV1]);
            expect(verificationRequest1.ourSupportedMethods).toStrictEqual([VerificationMethod.SasV1, VerificationMethod.ReciprocateV1]);

            expect(verificationRequest2.theirSupportedMethods).toStrictEqual([VerificationMethod.SasV1, VerificationMethod.ReciprocateV1]);
            expect(verificationRequest2.ourSupportedMethods).toStrictEqual([VerificationMethod.SasV1, VerificationMethod.ReciprocateV1]);
        });

        // SAS verification for the second machine.
        let sas2;

        test('can start a SAS verification (`m.key.verification.start`)', async () => {
            // Let's start a SAS verification, from `m2` for example.
            [sas2, outgoingVerificationRequest] = await verificationRequest2.startSas();
            expect(sas2).toBeInstanceOf(Sas);

            expect(sas2.userId.toString()).toStrictEqual(userId2.toString());
            expect(sas2.deviceId.toString()).toStrictEqual(deviceId2.toString());
            expect(sas2.otherUserId.toString()).toStrictEqual(userId1.toString());
            expect(sas2.otherDeviceId.toString()).toStrictEqual(deviceId1.toString());
            expect(sas2.flowId).toStrictEqual(flowId);
            expect(sas2.roomId).toBeUndefined();
            expect(sas2.supportsEmoji()).toStrictEqual(false);
            expect(sas2.startedFromRequest()).toStrictEqual(true);
            expect(sas2.isSelfVerification()).toStrictEqual(false);
            expect(sas2.haveWeConfirmed()).toStrictEqual(false);
            expect(sas2.hasBeenAccepted()).toStrictEqual(false);
            expect(sas2.cancelInfo()).toBeUndefined();
            expect(sas2.weStarted()).toStrictEqual(false);
            expect(sas2.timedOut()).toStrictEqual(false);
            expect(sas2.canBePresented()).toStrictEqual(false);
            expect(sas2.isDone()).toStrictEqual(false);
            expect(sas2.isCancelled()).toStrictEqual(false);
            expect(sas2.emoji()).toBeUndefined();
            expect(sas2.emojiIndex()).toBeUndefined();
            expect(sas2.decimals()).toBeUndefined();

            expect(outgoingVerificationRequest).toBeInstanceOf(ToDeviceRequest);

            outgoingVerificationRequest = JSON.parse(outgoingVerificationRequest.body);
            expect(outgoingVerificationRequest.event_type).toStrictEqual('m.key.verification.start');

            const toDeviceEvents = {
                events: [{
                    sender: userId2.toString(),
                    type: outgoingVerificationRequest.event_type,
                    content: outgoingVerificationRequest.messages[userId1.toString()][deviceId1.toString()],
                }],
            };

            // Let's send the SAS start to `m1`.
            await m1.receiveSyncChanges(JSON.stringify(toDeviceEvents), new DeviceLists(), new Map(), new Set());
        });

        // SAS verification for the second machine.
        let sas1;

        test('can fetch and accept an ongoing SAS verification (`m.key.verification.accept`)', async () => {
            // Let's fetch the ongoing SAS verification.
            sas1 = await m1.getVerification(userId2, flowId);

            expect(sas1).toBeInstanceOf(Sas);

            expect(sas1.userId.toString()).toStrictEqual(userId1.toString());
            expect(sas1.deviceId.toString()).toStrictEqual(deviceId1.toString());
            expect(sas1.otherUserId.toString()).toStrictEqual(userId2.toString());
            expect(sas1.otherDeviceId.toString()).toStrictEqual(deviceId2.toString());
            expect(sas1.flowId).toStrictEqual(flowId);
            expect(sas1.roomId).toBeUndefined();
            expect(sas1.startedFromRequest()).toStrictEqual(true);
            expect(sas1.isSelfVerification()).toStrictEqual(false);
            expect(sas1.haveWeConfirmed()).toStrictEqual(false);
            expect(sas1.hasBeenAccepted()).toStrictEqual(false);
            expect(sas1.cancelInfo()).toBeUndefined();
            expect(sas1.weStarted()).toStrictEqual(true);
            expect(sas1.timedOut()).toStrictEqual(false);
            expect(sas1.canBePresented()).toStrictEqual(false);
            expect(sas1.isDone()).toStrictEqual(false);
            expect(sas1.isCancelled()).toStrictEqual(false);
            expect(sas1.emoji()).toBeUndefined();
            expect(sas1.emojiIndex()).toBeUndefined();
            expect(sas1.decimals()).toBeUndefined();

            // Let's accept thet SAS start request.
            let outgoingVerificationRequest = sas1.accept();
            expect(outgoingVerificationRequest).toBeInstanceOf(ToDeviceRequest);

            outgoingVerificationRequest = JSON.parse(outgoingVerificationRequest.body);
            expect(outgoingVerificationRequest.event_type).toStrictEqual('m.key.verification.accept');

            const toDeviceEvents = {
                events: [{
                    sender: userId1.toString(),
                    type: outgoingVerificationRequest.event_type,
                    content: outgoingVerificationRequest.messages[userId2.toString()][deviceId2.toString()],
                }],
            };

            // Let's send the SAS accept to `m2`.
            await m2.receiveSyncChanges(JSON.stringify(toDeviceEvents), new DeviceLists(), new Map(), new Set());
        });

        test('emojis are supported by both sides', () => {
            expect(sas1.supportsEmoji()).toStrictEqual(true);
            expect(sas2.supportsEmoji()).toStrictEqual(true);
        });

        test('one side sends verification key (`m.key.verification.key`)', async () => {
            // Let's send the verification keys from `m2` to `m1`.
            const outgoingRequests = await m2.outgoingRequests();
            let toDeviceRequest = outgoingRequests.find((request) => request.type == RequestType.ToDevice);

            expect(toDeviceRequest).toBeInstanceOf(ToDeviceRequest);
            const toDeviceRequestId = toDeviceRequest.id;
            const toDeviceRequestType = toDeviceRequest.type;

            toDeviceRequest = JSON.parse(toDeviceRequest.body);
            expect(toDeviceRequest.event_type).toStrictEqual('m.key.verification.key');

            const toDeviceEvents = {
                events: [{
                    sender: userId2.toString(),
                    type: toDeviceRequest.event_type,
                    content: toDeviceRequest.messages[userId1.toString()][deviceId1.toString()],
                }],
            };

            // Let's send te SAS key to `m1`.
            await m1.receiveSyncChanges(JSON.stringify(toDeviceEvents), new DeviceLists(), new Map(), new Set());

            m2.markRequestAsSent(toDeviceRequestId, toDeviceRequestType, '{}');
        });

        test('other side sends back verification key (`m.key.verification.key`)', async () => {
            // Let's send the verification keys from `m1` to `m2`.
            const outgoingRequests = await m1.outgoingRequests();
            let toDeviceRequest = outgoingRequests.find((request) => request.type == RequestType.ToDevice);

            expect(toDeviceRequest).toBeInstanceOf(ToDeviceRequest);
            const toDeviceRequestId = toDeviceRequest.id;
            const toDeviceRequestType = toDeviceRequest.type;

            toDeviceRequest = JSON.parse(toDeviceRequest.body);
            expect(toDeviceRequest.event_type).toStrictEqual('m.key.verification.key');

            const toDeviceEvents = {
                events: [{
                    sender: userId1.toString(),
                    type: toDeviceRequest.event_type,
                    content: toDeviceRequest.messages[userId2.toString()][deviceId2.toString()],
                }],
            };

            // Let's send te SAS key to `m2`.
            await m2.receiveSyncChanges(JSON.stringify(toDeviceEvents), new DeviceLists(), new Map(), new Set());

            m1.markRequestAsSent(toDeviceRequestId, toDeviceRequestType, '{}');
        });

        test('emojis match from both sides', () => {
            const emojis1 = sas1.emoji();
            const emojiIndexes1 = sas1.emojiIndex();
            const emojis2 = sas2.emoji();
            const emojiIndexes2 = sas2.emojiIndex();

            expect(emojis1).toHaveLength(7);
            expect(emojiIndexes1).toHaveLength(emojis1.length);
            expect(emojis2).toHaveLength(emojis1.length);
            expect(emojiIndexes2).toHaveLength(emojis1.length);

            const isEmoji = /(\u00a9|\u00ae|[\u2000-\u3300]|\ud83c[\ud000-\udfff]|\ud83d[\ud000-\udfff]|\ud83e[\ud000-\udfff])/;

            for (const [emoji1, emojiIndex1, emoji2, emojiIndex2] of zip(emojis1, emojiIndexes1, emojis2, emojiIndexes2)) {
                expect(emoji1).toBeInstanceOf(Emoji);
                expect(emoji1.symbol).toMatch(isEmoji);
                expect(emoji1.description).toBeTruthy();

                expect(emojiIndex1).toBeGreaterThanOrEqual(0);
                expect(emojiIndex1).toBeLessThanOrEqual(63);

                expect(emoji2).toBeInstanceOf(Emoji);
                expect(emoji2.symbol).toStrictEqual(emoji1.symbol);
                expect(emoji2.description).toStrictEqual(emoji1.description);

                expect(emojiIndex2).toStrictEqual(emojiIndex1);
            }
        });

        test('decimals match from both sides', () => {
            const decimals1 = sas1.decimals();
            const decimals2 = sas2.decimals();

            expect(decimals1).toHaveLength(3);
            expect(decimals2).toHaveLength(decimals1.length);

            const isDecimal = /^[0-9]{4}$/;

            for (const [decimal1, decimal2] of zip(decimals1, decimals2)) {
                expect(decimal1.toString()).toMatch(isDecimal);

                expect(decimal2).toStrictEqual(decimal1);
            }
        });

        test('can confirm keys match (`m.key.verification.mac`)', async () => {
            // `m1` confirms.
            const [outgoingVerificationRequests, signatureUploadRequest] = await sas1.confirm();

            expect(signatureUploadRequest).toBeUndefined();
            expect(outgoingVerificationRequests).toHaveLength(1);

            let outgoingVerificationRequest = outgoingVerificationRequests[0];

            expect(outgoingVerificationRequest).toBeInstanceOf(ToDeviceRequest);

            outgoingVerificationRequest = JSON.parse(outgoingVerificationRequest.body);
            expect(outgoingVerificationRequest.event_type).toStrictEqual('m.key.verification.mac');

            const toDeviceEvents = {
                events: [{
                    sender: userId1.toString(),
                    type: outgoingVerificationRequest.event_type,
                    content: outgoingVerificationRequest.messages[userId2.toString()][deviceId2.toString()],
                }],
            };

            // Let's send te SAS confirmation to `m2`.
            await m2.receiveSyncChanges(JSON.stringify(toDeviceEvents), new DeviceLists(), new Map(), new Set());
        });

        test('can confirm back keys match (`m.key.verification.done`)', async () => {
            // `m2` confirms.
            const [outgoingVerificationRequests, signatureUploadRequest] = await sas2.confirm();

            expect(signatureUploadRequest).toBeUndefined();
            expect(outgoingVerificationRequests).toHaveLength(2);

            // `.mac`
            {
                let outgoingVerificationRequest = outgoingVerificationRequests[0];

                expect(outgoingVerificationRequest).toBeInstanceOf(ToDeviceRequest);

                outgoingVerificationRequest = JSON.parse(outgoingVerificationRequest.body);
                expect(outgoingVerificationRequest.event_type).toStrictEqual('m.key.verification.mac');

                const toDeviceEvents = {
                    events: [{
                        sender: userId2.toString(),
                        type: outgoingVerificationRequest.event_type,
                        content: outgoingVerificationRequest.messages[userId1.toString()][deviceId1.toString()],
                    }],
                };

                // Let's send te SAS confirmation to `m1`.
                await m1.receiveSyncChanges(JSON.stringify(toDeviceEvents), new DeviceLists(), new Map(), new Set());
            }

            // `.done`
            {
                let outgoingVerificationRequest = outgoingVerificationRequests[1];

                expect(outgoingVerificationRequest).toBeInstanceOf(ToDeviceRequest);

                outgoingVerificationRequest = JSON.parse(outgoingVerificationRequest.body);
                expect(outgoingVerificationRequest.event_type).toStrictEqual('m.key.verification.done');

                const toDeviceEvents = {
                    events: [{
                        sender: userId2.toString(),
                        type: outgoingVerificationRequest.event_type,
                        content: outgoingVerificationRequest.messages[userId1.toString()][deviceId1.toString()],
                    }],
                };

                // Let's send te SAS done to `m1`.
                await m1.receiveSyncChanges(JSON.stringify(toDeviceEvents), new DeviceLists(), new Map(), new Set());
            }
        });

        test('can send final done (`m.key.verification.done`)', async () => {
            const outgoingRequests = await m1.outgoingRequests();
            expect(outgoingRequests).toHaveLength(3);

            let toDeviceRequest = outgoingRequests.find((request) => request.type == RequestType.ToDevice);

            expect(toDeviceRequest).toBeInstanceOf(ToDeviceRequest);
            const toDeviceRequestId = toDeviceRequest.id;
            const toDeviceRequestType = toDeviceRequest.type;

            toDeviceRequest = JSON.parse(toDeviceRequest.body);
            expect(toDeviceRequest.event_type).toStrictEqual('m.key.verification.done');

            const toDeviceEvents = {
                events: [{
                    sender: userId1.toString(),
                    type: toDeviceRequest.event_type,
                    content: toDeviceRequest.messages[userId2.toString()][deviceId2.toString()],
                }],
            };

            // Let's send te SAS key to `m2`.
            await m2.receiveSyncChanges(JSON.stringify(toDeviceEvents), new DeviceLists(), new Map(), new Set());

            m1.markRequestAsSent(toDeviceRequestId, toDeviceRequestType, '{}');
        });

        test('can see if verification is done', () => {
            expect(verificationRequest1.isDone()).toStrictEqual(true);
            expect(verificationRequest2.isDone()).toStrictEqual(true);

            expect(sas1.isDone()).toStrictEqual(true);
            expect(sas2.isDone()).toStrictEqual(true);
        });
    });

    describe('QR Code', () => {
    });
});

describe('VerificationMethod', () => {
    test('has the correct variant values', () => {
        expect(VerificationMethod.SasV1).toStrictEqual(0);
        expect(VerificationMethod.QrCodeScanV1).toStrictEqual(1);
        expect(VerificationMethod.QrCodeShowV1).toStrictEqual(2);
        expect(VerificationMethod.ReciprocateV1).toStrictEqual(3);
    });
});
