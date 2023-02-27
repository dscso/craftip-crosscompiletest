// unit tests for the library

use crate::{HelloPacket, VarInt};

struct TestHelloPacket {
    name: String,
    buffer: Vec<u8>,
    packet: HelloPacket,
}

struct TestVarInt {
    buffer: Vec<u8>,
    value: VarInt,
}

#[cfg(test)]
mod tests {
    use crate::test::{TestHelloPacket, TestVarInt};
    use crate::{HelloPacket, Packet, VarInt};

    #[test]
    fn test_hello_packet_ping() {
        let test_vector = vec![
            TestHelloPacket {
                name: "pring with long hostname".to_string(),
                buffer: vec![
                    254, 1, 250, 0, 11, 0, 77, 0, 67, 0, 124, 0, 80, 0, 105, 0, 110, 0, 103, 0, 72,
                    0, 111, 0, 115, 0, 116, 0, 133, 73, 0, 63, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97,
                    0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0,
                    97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0,
                    97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0,
                    97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0,
                    97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0,
                    97, 0, 97, 0, 97, 0, 0, 99, 221,
                ],
                packet: HelloPacket {
                    length: 162,
                    id: 0,
                    version: 73,
                    hostname: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                        .parse()
                        .unwrap(),
                    port: 25565,
                },
            },
            TestHelloPacket {
                name: "pring with short hostname".to_string(),
                buffer: vec![
                    254, 1, 250, 0, 11, 0, 77, 0, 67, 0, 124, 0, 80, 0, 105, 0, 110, 0, 103, 0, 72,
                    0, 111, 0, 115, 0, 116, 0, 11, 73, 0, 2, 0, 104, 0, 105, 0, 0, 99, 221,
                ],
                packet: HelloPacket {
                    length: 40,
                    id: 0,
                    version: 73,
                    hostname: "hi".parse().unwrap(),
                    port: 25565,
                },
            },
            TestHelloPacket {
                name: "connect with long hostname".to_string(),
                buffer: vec![
                    2, 73, 0, 11, 0, 80, 0, 101, 0, 110, 0, 110, 0, 101, 0, 114, 0, 81, 0, 117, 0,
                    101, 0, 101, 0, 110, 0, 63, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0,
                    97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0,
                    97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0,
                    97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0,
                    97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0,
                    97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0, 97, 0,
                    97, 0, 0, 99, 221,
                ],
                packet: HelloPacket {
                    length: 158,
                    id: 0,
                    version: 0,
                    hostname: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                        .parse()
                        .unwrap(),
                    port: 25565,
                },
            },
            TestHelloPacket {
                name: "connect with short hostname".to_string(),
                buffer: vec![
                    2, 73, 0, 11, 0, 80, 0, 101, 0, 110, 0, 110, 0, 101, 0, 114, 0, 81, 0, 117, 0,
                    101, 0, 101, 0, 110, 0, 9, 0, 108, 0, 111, 0, 99, 0, 97, 0, 108, 0, 104, 0,
                    111, 0, 115, 0, 116, 0, 0, 99, 221,
                ],
                packet: HelloPacket {
                    length: 50,
                    id: 0,
                    version: 0,
                    hostname: "localhost".parse().unwrap(),
                    port: 25565,
                },
            },
            TestHelloPacket {
                name: "connect with too long buffer".to_string(),
                buffer: vec![
                    2, 73, 0, 11, 0, 80, 0, 101, 0, 110, 0, 110, 0, 101, 0, 114, 0, 81, 0, 117, 0,
                    101, 0, 101, 0, 110, 0, 9, 0, 108, 0, 111, 0, 99, 0, 97, 0, 108, 0, 104, 0,
                    111, 0, 115, 0, 116, 0, 0, 99, 221, 0, 0, 0, 0, 1, 2, 3, 4,
                ],
                packet: HelloPacket {
                    length: 50,
                    id: 0,
                    version: 0,
                    hostname: "localhost".parse().unwrap(),
                    port: 25565,
                },
            },
        ];
        test_vector.iter().for_each(|test| {
            println!("Testing {}...", test.name);
            let packet = HelloPacket::new(Packet {
                length: test.buffer.len(),
                data: test.buffer.clone(),
            })
            .unwrap();
            assert_eq!(packet, test.packet);
        });
    }
    #[test]
    fn test_varint() {
        let test_vector = vec![
            TestVarInt {
                buffer: vec![0x00],
                value: VarInt { value: 0, size: 1 },
            },
            TestVarInt {
                buffer: vec![0x01],
                value: VarInt { value: 1, size: 1 },
            },
            TestVarInt {
                buffer: vec![0x7f],
                value: VarInt {
                    value: 127,
                    size: 1,
                },
            },
            TestVarInt {
                buffer: vec![0x80, 0x01],
                value: VarInt {
                    value: 128,
                    size: 2,
                },
            },
            /*TestVarInt {
                buffer: vec![ 0xff, 0xff, 0xff, 0xff, 0x07 ],
                value: VarInt { value:  2147483647 , size: 5 },
            },
            TestVarInt {
                buffer: vec![ 0xff, 0xff, 0xff, 0xff, 0x0f ],
                value: VarInt { value:  -1 , size: 5 },
            },
            TestVarInt {
                buffer: vec![ 0x80, 0x80, 0x80, 0x80, 0x08 ],
                value: VarInt { value:   -2147483648  , size: 5 },
            },*/
        ];
        test_vector.iter().for_each(|test| {
            println!("Testing {:?}...", test.value.value);
            let value = VarInt::new(&*test.buffer.clone(), 0).unwrap();
            assert_eq!(value, test.value);
        });
    }
}
