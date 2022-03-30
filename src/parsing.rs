#[cfg(test)]
mod tests {
    use bytes::{Buf, BufMut, Bytes, BytesMut};
    use tempfile;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{UnixListener, UnixStream};
    use tokio_util::codec::{Decoder, Encoder};

    const JSON: &str = r#"
{
    "code": 200,
    "success": true,
    "payload": {
        "features": [
            "awesome",
            "easyAPI",
            "lowLearningCurve"
        ]
    }
}
"#;
    use json::{array, object, parse};

    #[tokio::test]
    async fn test_json() {
        let parsed = parse(JSON).unwrap();

        let instantiated = object! {
            "code" => 200,
            "success" => true,
            "payload" => object!{
                "features" => array![
                    "awesome",
                    "easyAPI",
                    "lowLearningCurve"
                ]
            }
        };

        assert_eq!(parsed, instantiated);
    }

    #[tokio::test]
    async fn test_bytes() -> () {
        let mut mem = Bytes::from("Hello world");
        let a = mem.slice(0..5);

        assert_eq!(a, "Hello");

        let b = mem.split_to(6);

        assert_eq!(mem, "world");
        assert_eq!(b, "Hello ");
    }

    #[tokio::test]
    async fn read_sockets() -> std::io::Result<()> {
        let dir = tempfile::Builder::new()
            .prefix("tokio-uds-tests")
            .tempdir()
            .unwrap();
        let sock_path = dir.path().join("connect.sock");
        let (mut server, _): (UnixStream, _) = UnixListener::bind(&sock_path)?.accept().await?;
        let mut client = UnixStream::connect(&sock_path).await?;

        // write a node to the client.
        let _ = client.write_all(b"hello world");

        // Read it back from the server.
        let mut buf = vec![];
        let _ = server.read_to_end(&mut buf).await?;

        Ok(())
    }

    #[tokio::test]
    async fn read_object_frames() {
        struct JsonMessage {
            msgLen: usize,
            json: String,
        }
        struct JsonMessageCodec;

        impl Decoder for JsonMessageCodec {
            type Item = JsonMessage;
            type Error = std::io::Error;

            fn decode(
                &mut self,
                src: &mut BytesMut,
            ) -> Result<Option<JsonMessage>, std::io::Error> {
                if src.len() < 4 {
                    return Ok(None);
                }
                let len: usize = src.get_u32() as usize;
                if src.len() < len + 4 {
                    // Allocate some more space
                    src.reserve(len + 4 - src.len());
                    return Ok(None);
                }

                let data = src[4..len + 4].to_vec();
                src.advance(4 + len); // Now src does not contain this stuff.
                match String::from_utf8(data) {
                    Ok(s) => Ok(Some(JsonMessage {
                        msgLen: len,
                        json: s,
                    })),
                    Err(utf8_error) => Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        utf8_error.utf8_error(),
                    )),
                }
            }
        }

        impl Encoder<JsonMessage> for JsonMessageCodec {
            type Error = std::io::Error;
            fn encode(
                &mut self,
                msg: JsonMessage,
                dst: &mut BytesMut,
            ) -> Result<(), std::io::Error> {
                dst.reserve(4 + msg.msgLen);
                dst.put_u32(msg.msgLen as u32);
                dst.put_slice(msg.json.as_bytes());
                Ok(())
            }
        }
    }
}
