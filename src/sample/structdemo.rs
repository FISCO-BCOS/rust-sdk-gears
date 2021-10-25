#![allow(
    clippy::unreadable_literal,
    clippy::upper_case_acronyms,
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    overflowing_literals,
    unused_imports,
    unused_results,
    unused_variables
)]
use std::time::Duration;
use crate::bcossdk::bcossdk::BcosSDK;
use crate::bcossdk::contractabi::ContractABI;
use crate::bcossdk::kisserror::KissError;
use crate::bcossdk::{bcossdkquery, fileutils};
use std::thread;
use serde_json::{json, Value as JsonValue};
use crate::bcossdk::contracthistory::ContractHistory;
use crate::bcossdk::bcossdkquery::json_hextoint;
use crate::bcossdk::cli_common::{Cli};
use crate::console::console_utils::display_transaction_receipt;
use fisco_bcos_rust_gears_sdk::bcossdk::liteutils;
use ethabi::{ParamType, Token};
use fisco_bcos_rust_gears_sdk::bcossdk::abi_tokenizer::{ABILenientTokenizer, ABITokenizer};

pub fn demo_deploy(bcossdk: &mut BcosSDK, contract:&ContractABI) -> Result<String,KissError>
{
    let contract_name = "TestStruct";
    let compileres  = BcosSDK::compile(contract_name,&bcossdk.config.configfile.as_ref().unwrap().as_str());
    println!("compile result:{:?}",compileres);

    let binfile = format!("{}/{}.bin",bcossdk.config.contract.contractpath,contract_name.to_string());
    let v = bcossdk.deploy_file(binfile.as_str(),"");
    println!("request response {:?}", v);
    let response = v.unwrap();
    let txhash = response["result"].as_str().unwrap();
    let recepitresult = bcossdk.try_getTransactionReceipt( txhash,3,false);
    println!("receipt {:?}",recepitresult);
    let receipt = recepitresult.unwrap();
    let addr:String = receipt["result"]["contractAddress"].as_str().unwrap().to_string();
    let blocknum = json_hextoint(&receipt["result"]["blockNumber"]).unwrap();
    println!("deploy contract on block {}",blocknum);
    let history_file = ContractHistory::history_file(bcossdk.config.contract.contractpath.as_str());
    let res = ContractHistory::save_to_file(history_file.as_str(),"NeedInit",addr.as_str(),blocknum as u32);
    Ok(addr)
}


pub fn test_split_param()
{
    let paramstr = "a,b,c";
    let res = liteutils::split_param(paramstr);
    println!("{:?}",res);
    let paramstr = "[11,22,33]";
    let res = liteutils::split_param(paramstr);
    println!("{:?}",res);
    let paramstr = "(alice,23),(bob,45)";
    let res = liteutils::split_param(paramstr);
    println!("{:?}",res);
}

pub fn demo(cli:&Cli)->Result<(),KissError>
{
    //test_split_param();
    //return Ok(());
    let mut bcossdk = BcosSDK::new_from_config(cli.default_configfile().as_str()).unwrap();
    //println!("{:?}",bcossdk.getNodeInfo());
    let contract = ContractABI::new_by_name("TestStruct",
                                        bcossdk.config.contract.contractpath.as_str(),
                                        &bcossdk.hashtype).unwrap();
    let address =  demo_deploy(&mut bcossdk,&contract).unwrap();
    println!("address = {:?}",address);

    println!("\n>>>>>>>>>>>>>>>>>>>>demo  call get");
    let callvalue = bcossdk.call(&contract,address.as_str(),"getUser",vec!("alice".to_string()).as_slice()).unwrap();
    println!("callvalue:{:?}",callvalue);
    let output = callvalue["result"]["output"].as_str().unwrap();


    let decodereuslt = contract.decode_output_byname("getUser", output);
    println!("getUser result{:?}",decodereuslt);

    println!("\n>>>>>>>>>>>>>>>>>>>>demo  addUser");
    let param =vec!("(frank,27)".to_string());
    let txres = bcossdk.sendRawTransactionGetReceipt(&contract,address.as_str(),"addUser",param.as_slice());
    println!("send tx result {:?}",&txres);
    display_transaction_receipt(&txres.unwrap(),&Option::from(&contract),&bcossdk.config);

    println!("\n-----------------addbyname--------------------------\n");
    let param =vec!("irisname".to_string(),"(iris,16)".to_string());
    let txres = bcossdk.sendRawTransactionGetReceipt(&contract,address.as_str(),"addbyname",param.as_slice());
    println!("send tx result {:?}",&txres);
    display_transaction_receipt(&txres.unwrap(),&Option::from(&contract),&bcossdk.config);



    println!("\n-----------------addUsers--------------------------\n");

    /*rust ethabi库对tuple数组的支持有问题，见ethabi的src/token/mod.rs的tokenize_array方法
        只是简单的按,分隔，类似"[(frank，85),(grant，55)]"这样在tuple里包含,的，就出错了。
        有待修改ethabi源代码或自行实现对tuple数组的解析来解决
        目前暂时不支持结构体数组
        同理，event，output里的解析也有问题
     */
    //let users =vec!("(fra\"nk,27)".to_string(),"(grant,55)".to_string(),"(kent'sz,11)".to_string(),);
    //let strdata = ContractABI::array_to_param(&users);
    let strdata = "[(frank,23),(grant55,77)]".to_string();
    println!("strdata {}",strdata);
    let param = vec!(strdata);
    println!("users param {:?}",param);
    let txres = bcossdk.sendRawTransactionGetReceipt(&contract,address.as_str(),"addUsers",param.as_slice())?;
    //println!("send tx result {}",&txres.unwrap().to_string());
    display_transaction_receipt(&txres,&Option::from(&contract),&bcossdk.config);

    println!("\n-----------------addUser with param tokens--------------------------\n");
    /*演示先将参数解析为token，直接传入去调用合约
    借助ContractABI, ABILenientTokenizer等工具精细化控制参数的解析。
    这种方式适合对合约的接口非常熟悉，可以自行拼装参数定义的使用者
    且参数里有特殊字符，不适合直接传字符串进行解析时，可以使用这种编程模式
    复杂结构很绕，应先熟读相关代码，原则上应保持合约接口尽量简单，最多到数组和结构体，不要搞嵌套
    */
    //按abi构造tuple的字段类型列表
    let user_param_type = vec!(Box::new(ParamType::String), Box::new(ParamType::Uint(256)));
    //这是数据,纯字符串数组，里面包含了特殊字符，确保特殊字符可以上链
    let user_in_str = vec!("pet\"288".to_string(), "314".to_string());
    //结合数据和类型，编码成token数组
    let user_in_token = ABILenientTokenizer::tokenize_struct_by_str_array(&user_in_str, &user_param_type).unwrap();
    let tupletoken = Token::Tuple(user_in_token); //将多个字段打入tuple token对象
    let result = bcossdk.sendRawTransactionGetReceiptWithTokenParam(&contract, address.as_str(), "addUser", &[tupletoken]);
    display_transaction_receipt(&result.unwrap(),&Option::from(&contract),&bcossdk.config);


    println!("\n-----------------addUsers (multi) with param tokens--------------------------\n");
    //按abi构造tuple的字段类型列表
    let user_param_type = vec!(Box::new(ParamType::String), Box::new(ParamType::Uint(256)));
    //addusers的第一个参数是user数组
    let mut users :Vec<Token> =vec!();

    //这是数据,纯字符串数组
    let user_in_str = vec!("rude988".to_string(), "3314".to_string());
    let user_in_token = ABILenientTokenizer::tokenize_struct_by_str_array(&user_in_str, &user_param_type).unwrap();
    let tupletoken = Token::Tuple(user_in_token); //将多个字段打入tuple token对象
    users.push(tupletoken); //增加第一个user

    //第二个user的数据，纯字符串数组
    let user_in_str  = vec!("queen354".to_string(),"618".to_string());
    let user_in_token = ABILenientTokenizer::tokenize_struct_by_str_array(&user_in_str, &user_param_type).unwrap();
    let tupletoken = Token::Tuple(user_in_token);//将多个字段打入tuple token
    users.push(tupletoken); //增加第二个user
    let arraytoken = Token::Array(users);  //将数组打入array token对象

    let result = bcossdk.sendRawTransactionGetReceiptWithTokenParam(&contract, address.as_str(), "addUsers", &[arraytoken]);
    display_transaction_receipt(&result.unwrap(),&Option::from(&contract),&bcossdk.config);


    Ok(())
}
