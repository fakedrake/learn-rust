#[cfg(test)]
mod tests {
    use bytes::{Buf, BufMut, Bytes, BytesMut};
    s
    use tempfile;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{UnixListener, UnixStream};
    use tokio_util::codec::{Decoder, Encoder, FramedWrite};
    use tokio::pin;
    use futures_sink::Sink;

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

    async fn new_server_client() -> std::io::Result<(UnixStream, UnixStream)> {
        let dir = tempfile::Builder::new()
            .prefix("tokio-uds-tests")
            .tempdir()
            .unwrap();
        let sock_path = dir.path().join("connect.sock");
        let sock = UnixListener::bind(&sock_path)?;
        let server_fut = sock.accept();
        let client_fut = UnixStream::connect(&sock_path);
        let ((server, _addr), client) = tokio::try_join!(server_fut, client_fut)?;
        Ok((server,client))
    }

    #[tokio::test]
    async fn test_read_sockets() -> std::io::Result<()> {
        let (mut server, mut client) = new_server_client().await?;
        // write a node to the client.For some reason the client needs
        // to be dropped first.
        let run_client = async {
            client.try_write(b"hello world");
            let ret = client.flush().await;
            drop(client);
            ret
        };

        let read_message = async {
            // Read it back from the server.
            let mut buf = vec![];
            server.read_to_end(&mut buf);
            buf
        };

        run_client.await;
        let res = read_message.await;
        assert_eq!(res, b"hello world");

        Ok(())
    }

    #[tokio::test]
    async fn read_object_frames() -> std::io::Result<()> {
        struct JsonMessage {
            json: String,
        }
        struct JsonMessageDec {
            msg_len: Option<usize>,
        }

        impl Decoder for JsonMessageDec {
            type Item = JsonMessage;
            type Error = std::io::Error;

            fn decode(
                &mut self,
                src: &mut BytesMut,
            ) -> Result<Option<JsonMessage>, std::io::Error> {
                // Read the first field.
                let len = match self.msg_len {
                    Some(len) => len,
                    None => {
                        if src.len() < 4 {
                            return Ok(None);
                        }
                        let len: usize = src.get_u32() as usize;
                        self.msg_len = Some(len);
                        len
                    }
                };

                if src.len() < len + 4 {
                    // Allocate some more space
                    src.reserve(len + 4 - src.len());
                    return Ok(None);
                }

                let data = src[4..len + 4].to_vec();
                src.advance(4 + len); // Now src does not contain this stuff.
                match String::from_utf8(data) {
                    Ok(s) => Ok(Some(JsonMessage {
                        json: s,
                    })),
                    Err(utf8_error) => Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        utf8_error.utf8_error(),
                    )),
                }
            }
        }
        struct JsonMessageEnc;
        impl Encoder<JsonMessage> for JsonMessageEnc {
            type Error = std::io::Error;
            fn encode(
                &mut self,
                msg: JsonMessage,
                dst: &mut BytesMut,
            ) -> Result<(), std::io::Error> {
                dst.reserve(4 + msg.json.len());
                dst.put_u32(msg.json.len() as u32);
                dst.put_slice(msg.json.as_bytes());
                Ok(())
            }
        }

        let (mut server, mut client) = new_server_client().await?;
        let mut framed_srv = JsonMessageDec{msg_len: None}.framed(server);
        let mut framed_clt = FramedWrite::new(client, JsonMessageEnc{});
        pin!(framed_clt);
        match framed_clt.poll_ready(cx) {
            Ready(_) => 1, // framed_clt.start_send(JsonMessage{json: r#"{"hello": 1}"#.to_string()}),
        }
        Ok(())
    }
}
